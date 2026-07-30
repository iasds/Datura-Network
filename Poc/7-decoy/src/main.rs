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
use node::{Destination, PacketSeen, run_destination, run_router};

const ROUTER_COUNT: usize = 6;
// multiple clients; show they each fan out to the same 8
const CLIENT_COUNT: usize = 4;

//full run product. lets main print it and tests assert on it
struct Demo {
    real_slot: usize,
    members: Vec<fanout::NodeId>, // the fixed public decoy set (8 node ids)
    sent: Vec<(usize, String)>,   // (client idx, wire tag)
    seen: Vec<PacketSeen>,        // one per client per dest
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
        let tx = report_tx.clone();
        workers.push(thread::spawn(move || {
            run_destination(listener, dest, client_count, tx)
        }));
    }
    drop(report_tx);

    // spread 8 dests over 6 routers
    // uneven on purpose: 8 dests don't divide evenly over 6 routers, so routers 0 and 1 cover 2 dest slots each (0&6, 1&7) while the rest cover 1.
    let mut router_fanout: Vec<Vec<u16>> = vec![Vec::new(); ROUTER_COUNT];
    //for 8 nodes: index of node, rand port
    for (slot, port) in dest_ports.iter().enumerate() {
        router_fanout[slot % ROUTER_COUNT].push(*port); // router = slot index modulo the amt of routers (slot%6), not an even split
    }

    // bind & spawn 6 routers, each relays 1 pkt per client
    let mut router_ports = Vec::with_capacity(ROUTER_COUNT); //6
    for forward_to in router_fanout {
        let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind router")?;
        router_ports.push(listener.local_addr()?.port());
        workers.push(thread::spawn(move || {
            run_router(listener, forward_to, client_count)
        }));
    }

    // every client seals its pkt to same hs pubkey and sends. no client has decoy set: only know hs pubkey
    let mut sent = Vec::with_capacity(client_count);
    for client in 0..client_count {
        let payload = format!("client {client} calling hidden service");
        let tag = fanout::deliver(&hs_public, payload.as_bytes(), &router_ports)?;
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
    for worker in workers {
        let _ = worker.join();
    }

    // results
    Ok(Demo {
        real_slot,
        members,
        sent,
        seen,
    })
}

fn report(demo: &Demo) -> Result<()> {
    let real_slot = demo.real_slot;
    let client_count = demo.sent.len();
    println!("hidden service is slot {real_slot}, decoy set is the same 8 for every client\n");

    // the fixed public decoy set: 8 node ids, no stored marker for which is real.
    println!("decoy set (public node ids, reused long-term):");
    for (slot, id) in demo.members.iter().enumerate() {
        let role = if slot == real_slot { " <- the hs" } else { "" };
        println!("  slot {slot}: {}{role}", hex::encode(&id[..8]));
    }
    println!();

    // per client: which of the 8 slots saw its pkt? must be all 8
    println!("client  tag       reached slots            opened by");
    println!("------  --------  -----------------------  ---------");
    for (client, tag) in &demo.sent {
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
            "{:>6}  {}  {:<23}  slot {}",
            client,
            tag,
            format!("{} ({}/{})", slots.join(","), reached.len(), DECOY_COUNT),
            opener
        );

        // still 8
        if reached.len() != DECOY_COUNT {
            bail!(
                "client {client} only reached {}/{} slots",
                reached.len(),
                DECOY_COUNT
            );
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
    println!("\nok: all {client_count} clients fanned out to the same {DECOY_COUNT} nodes.");
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
}
