mod tests;

pub struct OrderBook {
    pub price_floor: u64,
    pub bids: Vec<Bid>,
}

impl OrderBook {
    pub fn new(price_floor: u64) -> Self {
        OrderBook { bids: Vec::new(), price_floor }
    }

    pub fn clear_auction(&mut self) -> Vec<Bid> {
        self.bids.sort();
    }

    pub fn record_bid(&mut self, bid: Bid) -> Result<(),OrderError> {
        if bid.unit_price < self.price_floor {
            return Err(OrderError::BidTooLow(self.price_floor));
        }
        self.bids.push(bid);
    }
}
