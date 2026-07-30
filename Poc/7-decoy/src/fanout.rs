use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use x_wing::{DecapsulationKey, Decapsulator, EncapsulationKey, Kem, KeyExport, XWingKem};

use crate::envelope::{self, PACKET_SIZE};
use crate::node::wire_tag;

pub const DECOY_COUNT: usize = 8; //1 real + 7 decoy
// theoretically, could be advanced setting to increase, but min 8?

// prod: decoy node's hashring position = Poseidon(pallas_pk.x, pallas_pk.y) (apparently, kinda a wacky system, but makes sense for security),
// rn its the blake3 of the node's xwing public key
pub type NodeId = [u8; 32];

// makes the addr/id from xwing encap key
pub fn node_id(public_key: &EncapsulationKey) -> NodeId {
    *blake3::hash(public_key.to_bytes().as_slice()).as_bytes()
}

// 'identity' is the hs's xwing decap key. prod: client would resolve .dn from descriptor to get this, resolution not here bc pubkey handed directly for poc
// 'members' arw the decoys: 8, reused, public node addrs. The hs doesn't have decoys keys. the hs knows its own slot bc of id it can reproduce from their own key
pub struct HiddenService {
    pub identity: DecapsulationKey,
    pub members: Vec<NodeId>,
    pub real_slot: usize,
}

// persistence anchored to crate dir, not process CWD. CWD-relative paths regen file when run from different dirs, rotating decoys
fn crate_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name) // anchored to dir
}

// hidden service's own key: secret, stored seperate
fn hs_identity_path() -> PathBuf {
    crate_file("hs_identity.txt")
}

// the decoys: 8 public node ids, no secrets
fn decoy_set_path() -> PathBuf {
    crate_file("hs_decoy_set.txt")
}

// loads the hs key + decoy set off disk, making either one if its not there yet.
// real slot is never stored, its found by matching own id inside the set
pub fn load_or_make_hidden_service() -> Result<HiddenService> {
    let id_path = hs_identity_path(); // both separate fns for clarity
    let set_path = decoy_set_path();

    //check file, or regen
    let identity = if id_path.exists() {
        load_identity(&id_path).context("reloading hs identity")?
    } else {
        let (secret, _) = XWingKem::generate_keypair();
        save_identity(&id_path, &secret)?;
        secret
    };
    let self_id = node_id(identity.encapsulation_key()); // own id is identity generated into file as hs id

    //same check
    let members = if set_path.exists() {
        read_member_set(&set_path).context("reloading decoys")?
    } else {
        let members = make_member_set(self_id)?;
        write_member_set(&set_path, &members)?;
        members
    };

    let real_slot = members // set correct id by checking equal to own
        .iter()
        .position(|member| member == &self_id)
        .context("decoy set doesnt contain own node id")?;

    Ok(HiddenService {
        identity,
        members,
        real_slot,
    })
}

// Build temp hidden service in mem for tests. doesnt use hs_identity.txt / hs_decoy_set.txt
#[cfg(test)]
pub(crate) fn make_hidden_service() -> Result<HiddenService> {
    let (identity, _) = XWingKem::generate_keypair();
    let self_id = node_id(identity.encapsulation_key()); // no need to save either
    let members = make_member_set(self_id)?;
    let real_slot = members
        .iter()
        .position(|member| member == &self_id)
        .expect("set contains self");
    Ok(HiddenService {
        identity,
        members,
        real_slot,
    })
}

// Put own id at random slot and fill the other 7 with ids of decoys selected.
// decoy nodes' secret keys not generated or stored
fn make_member_set(self_id: NodeId) -> Result<Vec<NodeId>> {
    // random byte picks the real slot. mod 8 doesnt skew -> 256%8=0 (256 vals / 8 buckets = 32 each)
    let mut slot_pick = [0u8; 1];
    getrandom::getrandom(&mut slot_pick).map_err(|error| anyhow::anyhow!("rng: {error}"))?;
    let real_slot = (slot_pick[0] as usize) % DECOY_COUNT; //set slot to rand mod count

    let mut members = Vec::with_capacity(DECOY_COUNT); // decoy 'member' to max 
    //fill all slots
    for slot in 0..DECOY_COUNT {
        if slot == real_slot {
            members.push(self_id);
        } else {
            // id the hs recorded for decoy node, rnd here
            let mut decoy_id = [0u8; 32];
            getrandom::getrandom(&mut decoy_id).map_err(|error| anyhow::anyhow!("rng: {error}"))?;
            members.push(decoy_id);
        }
    }
    Ok(members)
}

fn load_identity(path: &Path) -> Result<DecapsulationKey> {
    let file_text = fs::read_to_string(path)?; // own id file
    let seed = hex::decode(file_text.trim())?;
    let seed: [u8; 32] = seed
        .as_slice()
        .try_into()
        .context("identity seed not 32 bytes")?;
    Ok(DecapsulationKey::from(seed))
}

//save hs key separetly
fn save_identity(path: &Path, secret: &DecapsulationKey) -> Result<()> {
    fs::write(path, hex::encode(secret.as_bytes())).context("writing hs identity")?;
    // hidden service's key is a secret, ensure in implement to lock it down
    Ok(())
}

//create decoy file with chosen members from public addr
fn write_member_set(path: &Path, members: &[NodeId]) -> Result<()> {
    let mut serialized = String::new();
    for id in members {
        serialized.push_str(&hex::encode(id));
        serialized.push('\n');
    }
    // 8 public ids
    fs::write(path, serialized).context("writing decoy set")?;
    Ok(())
}

fn read_member_set(path: &Path) -> Result<Vec<NodeId>> {
    let file_text = fs::read_to_string(path)?;
    let mut members = Vec::new();
    for line in file_text.lines().filter(|line| !line.trim().is_empty()) {
        let bytes = hex::decode(line.trim())?;
        let id: NodeId = bytes
            .as_slice()
            .try_into()
            .context("node id not 32 bytes")?;
        members.push(id);
    }
    //verify 8 total
    if members.len() != DECOY_COUNT {
        bail!(
            "decoy set has {} entries, expected {}",
            members.len(),
            DECOY_COUNT
        );
    }
    Ok(members)
}

// a client's send. client only knows hs public key. seals a pkt and pushes the same bytes to every router; the routers fan it out to the hs's fixed 8
// returns wire tag so main can confirm which slots saw client's pkt.
pub fn deliver(hs_public: &EncapsulationKey, payload: &[u8], routers: &[u16]) -> Result<String> {
    use std::io::Write;
    let packet = envelope::seal(hs_public, payload)?; //envelope.rs main function
    assert_eq!(packet.len(), PACKET_SIZE);

    for port in routers {
        let mut sock = TcpStream::connect(("127.0.0.1", *port))
            .with_context(|| format!("client dialing router {port}"))?;
        sock.write_all(&packet)?;
    }
    Ok(wire_tag(&packet))
}
