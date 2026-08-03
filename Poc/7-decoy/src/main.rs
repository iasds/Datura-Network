//! poc7, decoy destinations (module doc comment as required by contrib guideliness)
//! client seals one packet to a hidden service and fans out copies to the hs's fixed set
//! of 8 dests (1 real + 7 decoys). only the real dest can open it (x-wing + chacha20poly1305);
//! the other 7 get noise. every client uses the same 8, so a gpa sees 8 co-receivers instead of 1. a 1/8 anon set

//! see spec/specification.md "Decoy Destinations and Sources"

mod envelope;
mod fanout;
mod node;

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use x_wing::{Decapsulator, Kem, XWingKem};

use fanout::{DECOY_COUNT, HiddenService};
use node::{Destination, HopSeen, PacketSeen, Relay, run_destination, run_relay};

// every node copies onward to 2 (the spec/image-12) 8 leaves = 2 levels of relays above them,
// so the path is client -> hop1 -> hop2 -> dest, 3 hops
const FANOUT: usize = 2;
const HOP2_COUNT: usize = DECOY_COUNT / FANOUT; // 4
const HOP1_COUNT: usize = HOP2_COUNT / FANOUT; // 2
// multiple clients; show they each fan out to the same 8
const CLIENT_COUNT: usize = 4;

//full run product. lets main print it and tests assert on it
struct Demo {
    real_slot: usize,
    members: Vec<fanout::NodeId>, // the fixed public decoy set (8 node ids)
    sent: Vec<(usize, String)>,   // (client idx, wire tag)
    seen: Vec<PacketSeen>,        // one per client per dest
    hops: Vec<HopSeen>,           // one per client per relay
}

fn main() -> Result<()> {
    // hs' key + fixed public decoy set, reused across runs
    let hs = fanout::load_or_make_hidden_service()?; // checks for file, or regens; outputs HiddenService{identity,members,real_slot}
    let demo = run_fanout(hs, CLIENT_COUNT)?;
    report(&demo)
}

// takes a hidden service (key + the public decoy set), spins nodes, runs `client_count` clients, returns what the dests saw.
// main feeds to persisted hs, tests feed temp in-mem
fn run_fanout(hs: HiddenService, client_count: usize) -> Result<Demo> {
    // grab the real slot + hs pubkey before the identity key moves into a thread
    let HiddenService {
        identity,
        members,
        real_slot,
    } = hs;
    let hs_public = identity.encapsulation_key().clone();
    let mut hs_identity = Some(identity);

    let (report_tx, report_rx) = mpsc::channel::<PacketSeen>();
    let (hop_tx, hop_rx) = mpsc::channel::<HopSeen>();
    let mut workers = Vec::new();

    // bind & spawn 8 dests, each expects 1 pkt per client. the real slot runs hs with key; decoys are independant nodes, gets its own key and fails to open
    // hs-sealed traffic. decoy keys not persisted & not part of hs data
    let mut dest_ports = Vec::with_capacity(DECOY_COUNT); //8
    for slot in 0..DECOY_COUNT {
        let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind dest")?;
        dest_ports.push(listener.local_addr()?.port());

        // set to hs id if slot correct
        let secret = if slot == real_slot {
            hs_identity.take().expect("real slot assigned once")
        } else {
            let (throwaway, _) = XWingKem::generate_keypair(); // otherwise decoy gets its own unrelated key
            throwaway
        };
        let dest = Destination { slot, secret }; //hs id & secret
        let dest_report = report_tx.clone();
        workers.push(thread::spawn(move || {
            run_destination(listener, dest, client_count, dest_report)
        }));
    }
    drop(report_tx);

    // build tree up, each level is bound after its dest lvl, so it knows the ports to copy to
    // hop2 relay j feeds dests 2j & 2j+1, hop1 relay k feeds hop2 relays 2k & 2k+1. also, straight split: tree is a binary fan
    let hop2_ports = spawn_level(
        2,
        HOP2_COUNT,
        &dest_ports,
        client_count,
        &hop_tx,
        &mut workers,
    )?;
    let hop1_ports = spawn_level(
        1,
        HOP1_COUNT,
        &hop2_ports,
        client_count,
        &hop_tx,
        &mut workers,
    )?;
    drop(hop_tx);

    // every client seals its pkt to same hs pubkey and pushes into the 2 entry relays
    // no client has decoy set, it only knows hs pubkey
    let mut sent = Vec::with_capacity(client_count);
    for client in 0..client_count {
        let payload = format!("client {client} calling hidden service");
        let tag = fanout::deliver(&hs_public, payload.as_bytes(), &hop1_ports)?;
        sent.push((client, tag));
    }

    // return one report per client per dest
    let expected = client_count * DECOY_COUNT;
    let mut seen = Vec::with_capacity(expected);
    for _ in 0..expected {
        match report_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(packet) => seen.push(packet),
            Err(_) => bail!("missing reports, got {}/{}", seen.len(), expected),
        }
    }

    // and one per client per relay, so the hops are shown
    let expected_hops = client_count * (HOP1_COUNT + HOP2_COUNT);
    let mut hops = Vec::with_capacity(expected_hops);
    for _ in 0..expected_hops {
        match hop_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(hop) => hops.push(hop),
            Err(_) => bail!("missing hop reports, got {}/{}", hops.len(), expected_hops),
        }
    }

    for worker in workers {
        let _ = worker.join();
    }

    // results
    Ok(Demo {
        real_slot,
        members,
        sent,
        seen,
        hops,
    })
}

// make one level of relays. relay i takes the FANOUT ports out of next_hop_ports
// relay 0 takes ports[0..2], relay 1 takes ports[2..4] etc
// returns this lvl's ports so the lvl above can point at em
fn spawn_level(
    level: usize,
    count: usize,
    next_hop_ports: &[u16],
    client_count: usize,
    hop_tx: &mpsc::Sender<HopSeen>,
    workers: &mut Vec<thread::JoinHandle<()>>,
) -> Result<Vec<u16>> {
    // each relay takes FANOUT of them. problematic if DECOY_COUNT isn't a multiple of 4
    if next_hop_ports.len() != count * FANOUT {
        bail!(
            "level {level}: {count} relays x {FANOUT} = {} next hops needed, got {}",
            count * FANOUT,
            next_hop_ports.len()
        );
    }

    let mut ports = Vec::with_capacity(count);
    for index in 0..count {
        let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind relay")?;
        ports.push(listener.local_addr()?.port());

        let forward_to = next_hop_ports[index * FANOUT..(index + 1) * FANOUT].to_vec();
        let relay = Relay { level, index };
        let hop_report = hop_tx.clone();
        workers.push(thread::spawn(move || {
            run_relay(listener, relay, forward_to, client_count, hop_report)
        }));
    }
    Ok(ports)
}

// how many relays sit at a level: 2 at hop1, 4 at hop2
fn relays_at(level: usize) -> usize {
    if level == 1 { HOP1_COUNT } else { HOP2_COUNT }
}

fn report(demo: &Demo) -> Result<()> {
    let real_slot = demo.real_slot;
    let client_count = demo.sent.len();
    println!("hidden service slot: {real_slot}, decoy set is the same 8 for every client");

    // real branch. others look the same. this is printed to check the pkt went 3 hops
    let real_hop2: usize = real_slot / FANOUT;
    let real_hop1: usize = real_hop2 / FANOUT;
    println!(
        "path: client -> {HOP1_COUNT} relays -> {HOP2_COUNT} relays -> {DECOY_COUNT} dests, real branch is hop1 {real_hop1} -> hop2 {real_hop2} -> slot {real_slot}\n"
    );

    // the fixed public decoy set: 8 node ids, no stored marker for which is real.
    println!("decoy set (public node ids, reused long-term):");
    for (slot, id) in demo.members.iter().enumerate() {
        let role = if slot == real_slot { " <- the hs" } else { "" };
        println!("  slot {slot}: {}{role}", hex::encode(&id[..8]));
    }
    println!();

    // per client: same tag has to show up at hop1 and hop2 before ending on all 8 slots
    // same bytes at every level shows followed tree instead of client --> node
    println!("client  tag       hop1  hop2  reached slots            opened by");
    println!("------  --------  ----  ----  -----------------------  ---------");
    for (client, tag) in &demo.sent {
        let at_level = |level: usize| {
            demo.hops
                .iter()
                .filter(|h: &&HopSeen| h.level == level && &h.wire_tag == tag)
                .count()
        };
        let (hop1, hop2) = (at_level(1), at_level(2));

        let reached: BTreeSet<usize> = demo //which clients saw pkt
            .seen
            .iter()
            .filter(|p| &p.wire_tag == tag)
            .map(|p| p.slot)
            .collect();
        let opener = demo //which opened
            .seen
            .iter()
            .find(|p| &p.wire_tag == tag && p.opened)
            .map(|p| p.slot.to_string())
            .unwrap_or_else(|| "none".into());
        let slots: Vec<String> = reached.iter().map(|s| s.to_string()).collect();
        println!(
            "{:>6}  {}  {:>4}  {:>4}  {:<23}  slot {}",
            client,
            tag,
            hop1,
            hop2,
            format!("{} ({}/{})", slots.join(","), reached.len(), DECOY_COUNT),
            opener
        );

        // followed both relay levels
        if hop1 != HOP1_COUNT || hop2 != HOP2_COUNT {
            bail!(
                "client {client} pkt hit {hop1}/{HOP1_COUNT} hop1 and {hop2}/{HOP2_COUNT} hop2 relays"
            );
        }

        // still 8
        if reached.len() != DECOY_COUNT {
            bail!(
                "client {client} only reached {}/{} slots",
                reached.len(),
                DECOY_COUNT
            );
        }
    }

    // per relay: 1 pkt per client in, FANOUT copies out
    println!("\nlevel  relay  received  forwarded");
    println!("-----  -----  --------  ---------");
    for level in 1..=2 {
        for index in 0..relays_at(level) {
            let relay_hops: Vec<&HopSeen> = demo
                .hops
                .iter()
                .filter(|h| h.level == level && h.index == index)
                .collect();
            let forwarded: usize = relay_hops.iter().map(|h: &&HopSeen| h.forwarded).sum();
            println!(
                "{:>5}  {:>5}  {:>8}  {:>9}",
                level,
                index,
                relay_hops.len(),
                forwarded
            );

            if relay_hops.len() != client_count || forwarded != client_count * FANOUT {
                bail!(
                    "relay {level}/{index} misbehaved: {} recv, {forwarded} fwd",
                    relay_hops.len()
                );
            }
        }
    }

    // per slot: should have received 1 pkt from every client, and only real slot ever opened
    println!("\nslot   role   received  opened");
    println!("----   -----  --------  ------");
    for slot in 0..DECOY_COUNT {
        let slot_packets: Vec<&PacketSeen> = demo.seen.iter().filter(|p| p.slot == slot).collect();
        let opened = slot_packets.iter().filter(|p| p.opened).count();
        let role = if slot == real_slot { "real" } else { "decoy" };
        println!(
            "{:>4}   {:<5}  {:>8}  {:>6}",
            slot,
            role,
            slot_packets.len(),
            opened
        );

        let expect_opened = if slot == real_slot { client_count } else { 0 };
        if slot_packets.len() != client_count || opened != expect_opened {
            bail!(
                "slot {slot} misbehaved: {} recv, {} opened",
                slot_packets.len(),
                opened
            );
        }
    }

    // note: a gpa running many clients sees 8 receivers not 1 -> anonymity at 1/8 (unless decoy compromised, then 1/8-k)
    println!(
        "\nok: all {client_count} clients fanned out over 3 hops to the same {DECOY_COUNT} nodes."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demonstrates_all_clients_fan_out_to_same_eight() {
        // in-mem hs, doesnt read hs_identity.txt / hs_decoy_set.txt.
        let hs = fanout::make_hidden_service().unwrap();
        let clients = 4;
        let demo = run_fanout(hs, clients).unwrap();

        // every client's pkt landed on all 8 slots,  intersection stays 8, not 1
        for (client, tag) in &demo.sent {
            let reached: BTreeSet<usize> = demo
                .seen
                .iter()
                .filter(|p| &p.wire_tag == tag)
                .map(|p| p.slot)
                .collect();
            assert_eq!(
                reached.len(),
                DECOY_COUNT,
                "client {client} missed slots: {reached:?}"
            );
        }

        // only the real slot ever opened, and it opened all `clients` pkts
        for slot in 0..DECOY_COUNT {
            let slot_packets: Vec<&PacketSeen> =
                demo.seen.iter().filter(|p| p.slot == slot).collect();
            let opened = slot_packets.iter().filter(|p| p.opened).count();
            assert_eq!(slot_packets.len(), clients, "slot {slot} wrong recv count");
            let want = if slot == demo.real_slot { clients } else { 0 };
            assert_eq!(opened, want, "slot {slot} opened {opened}, wanted {want}");
        }
    }

    // check the pkt is seen at every level rather than the 8 endpoints
    #[test]
    fn traffic_walks_three_hops() {
        let hs: HiddenService = fanout::make_hidden_service().unwrap();
        let clients: usize = 2;
        let demo: Demo = run_fanout(hs, clients).unwrap();

        // every relay took 1 pkt and copied it to 2
        for level in 1..=2 {
            for index in 0..relays_at(level) {
                let relay_hops: Vec<&HopSeen> = demo
                    .hops
                    .iter()
                    .filter(|h: &&HopSeen| h.level == level && h.index == index)
                    .collect();
                assert_eq!(
                    relay_hops.len(),
                    clients,
                    "relay {level}/{index} wrong recv count"
                );
                for hop in relay_hops {
                    assert_eq!(hop.forwarded, FANOUT, "relay {level}/{index} fanned wrong");
                }
            }
        }

        // same bytes at hop1, hop2 and a dest: the packet went all 3 hops
        for (client, tag) in &demo.sent {
            for level in 1..=2 {
                assert!(
                    demo.hops
                        .iter()
                        .any(|h| h.level == level && &h.wire_tag == tag),
                    "client {client} pkt never seen at level {level}"
                );
            }
            assert!(
                demo.seen.iter().any(|p| &p.wire_tag == tag),
                "client {client} pkt never reached a dest"
            );
        }
    }
}
