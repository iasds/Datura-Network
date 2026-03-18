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
			("PUT", n) => Ok(Protocol::Put(usize::from_str(n).map_err(|_| ())?)),
			_ => Err(()),
		}
	}
}
