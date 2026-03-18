#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
	Knock,
	Put(usize),
	Get([u8; 32]),
}
