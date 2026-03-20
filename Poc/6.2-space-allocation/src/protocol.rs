use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
	Knock,
	Put(usize),
	Get([u8; 32]),
}

impl FromStr for Protocol {
	type Err = ();
	fn from_str(input: &str) -> Result<Protocol, Self::Err> {
		match input.split_once(' ').unwrap_or((input, "")) {
			("KNOCK", "") => Ok(Protocol::Knock),
			("PUT", n) => usize::from_str(n).map(Protocol::Put).map_err(|_| ()),
			("GET", dataid) => dataid[..32]
				.as_bytes()
				.try_into()
				.map(Protocol::Get)
				.map_err(|_| ()),
			_ => Err(()),
		}
	}
}
