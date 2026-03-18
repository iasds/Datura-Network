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
		match input {
			"KNOCK" => Ok(Protocol::Knock),
			_ => Err(()),
		}
	}
}
