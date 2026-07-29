#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
    Knock,
    Put(usize),
    Get([u8; 32]),
}

impl TryFrom<[u8; 36]> for Protocol {
    type Error = ();
    fn try_from(input: [u8; 36]) -> Result<Protocol, Self::Error> {
        match &input[..3] {
            b"KNO" => {
                if &input[3..5] == b"CK" {
                    Ok(Protocol::Knock)
                } else {
                    Err(())
                }
            }
            b"PUT" => Ok(Protocol::Put(usize::from_ne_bytes(
                input[4..12].try_into().unwrap(),
            ))),
            b"GET" => Ok(Protocol::Get(input[4..].try_into().unwrap())),
            _ => Err(()),
        }
    }
}
