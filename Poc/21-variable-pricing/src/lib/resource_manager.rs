use super::order_book::OrderBook;

pub struct ResourceManager {
    total_available: u64,
    total_allocated: u64,
    order_book: OrderBook,
}

impl ResourceManager {
    pub fn new(total_available: u64) -> Self {
        ResourceManager {
            total_available,
            total_allocated: 0,
            order_book: OrderBook::new(),
        }
    }
}
