/// Wire protocol for Datura circuit cells.
///
/// Each cell is fixed-size (CELL_LEN bytes):
///   [type: u8][circuit_id: u32 LE][payload: PAYLOAD_LEN bytes]
///
/// Cell types:
///   CREATE     – client → hop:   begin DH handshake; payload = client X25519 pubkey
///   CREATED    – hop → client:   complete DH handshake; payload = relay X25519 pubkey
///   EXTEND     – client → hop:   ask hop to extend circuit; payload = [addr_len:1][addr:N][pubkey:32]
///   EXTENDED   – hop → client:   extension complete; payload = next-hop X25519 pubkey
///   RELAY      – any direction:  507-byte onion-encrypted payload (no AEAD tag, stream cipher only)
///   DATA       – inner payload:  [len:2][data:N] — visible only after all layers are peeled
///   BRIDGE     – HS → RV relay:  link two circuit legs; payload = [client_circuit_id:4 LE]
///   INTRO      – HS → intro:     hidden service registers with the intro point
///   RENDEZVOUS – intro → HS:     intro forwards client's RV info; payload = [addr_len:1][rv_addr:N][circuit_id:4 LE]
///   CONNECT    – client → intro: request HS connection; payload = [addr_len:1][rv_addr:N][circuit_id:4 LE]
use std::io::{Read, Write};
use std::net::TcpStream;

pub const CELL_LEN: usize = 512;
pub const PAYLOAD_LEN: usize = CELL_LEN - 5; // 507 bytes

pub const TYPE_CREATE: u8 = 0x01;
pub const TYPE_CREATED: u8 = 0x02;
pub const TYPE_EXTEND: u8 = 0x03;
pub const TYPE_EXTENDED: u8 = 0x04;
pub const TYPE_RELAY: u8 = 0x05;
pub const TYPE_DATA: u8 = 0x06;
pub const TYPE_BRIDGE: u8 = 0x07;
pub const TYPE_INTRO: u8 = 0x08;
pub const TYPE_RENDEZVOUS: u8 = 0x09;
pub const TYPE_CONNECT: u8 = 0x0A;

#[derive(Clone)]
pub struct Cell {
    pub cell_type: u8,
    pub circuit_id: u32,
    pub payload: [u8; PAYLOAD_LEN],
}

impl Cell {
    pub fn new(cell_type: u8, circuit_id: u32) -> Self {
        Self {
            cell_type,
            circuit_id,
            payload: [0u8; PAYLOAD_LEN],
        }
    }

    pub fn to_bytes(&self) -> [u8; CELL_LEN] {
        let mut buf = [0u8; CELL_LEN];
        buf[0] = self.cell_type;
        buf[1..5].copy_from_slice(&self.circuit_id.to_le_bytes());
        buf[5..].copy_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(buf: &[u8; CELL_LEN]) -> Self {
        let mut payload = [0u8; PAYLOAD_LEN];
        payload.copy_from_slice(&buf[5..]);
        Self {
            cell_type: buf[0],
            circuit_id: u32::from_le_bytes(buf[1..5].try_into().unwrap()),
            payload,
        }
    }

    pub fn send(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        stream.write_all(&self.to_bytes())
    }

    pub fn recv(stream: &mut TcpStream) -> std::io::Result<Self> {
        let mut buf = [0u8; CELL_LEN];
        stream.read_exact(&mut buf)?;
        Ok(Self::from_bytes(&buf))
    }
}
