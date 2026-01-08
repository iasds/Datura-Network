use proptest::prelude::*
use uuid::Uuid;
use std::cmp::{Ord,Ordering};
use std::time::Instant;

pub struct Bid {
    pub unit_price: u64,
    pub amount: u64,
    pub id: Uuid,
    pub status: BidStatus,
    pub date: Instant,
}

pub enum BidStatus {
    New,
    Submitted,

    ///number of units purchases and per unit price
    PartiallyFulfilled((u64,u64)),
    
    ///unit price
    Fulfilled(u64),

    Unsuccessful(u64),
}

impl Bid {
    pub fn new(unit_price: u64, amount: u64) {
        Bid {
            unit_price,
            amount,
            id: Uuid::new_v4(),
            date: Instant::now(),
            status: BidStatus::New,
    }
}

impl Ord for Bid {
        // Required method
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.unit_price == other.unit_price {
            return Some(self.date.cmp(other.date));
        }
        else {
            Some(self.unit_price.cmp(other.unit_price))
        }
    }

    // Provided methods
    fn lt(&self, other: &Self) -> bool {
        self.unit_price < other.unit_price || self.date > other.date
    }
    fn le(&self, other: &Self) -> bool {
        self.unit_price <= other.unit_price || self.date >= other.date
    }
    fn gt(&self, other: &Self) -> bool {
        self.unit_price > other.unit_price || self.date < other.date
    }
    fn ge(&self, other: &Self) -> bool {
        self.unit_price >= other.unit_price || self.date <= other.date
    }
}

prop_compose! {

    fn arb_bid()(unit_price in any::u64, amount in any::u64) -> Bid {
        Bid::new(unit_price, amount)
    }
}

prop_compose! {
    fn arb_orderbook()(floor_price in any::u64) -> OrderBook {
        OrderBook::new(floor_price)
    }
}

proptest! {
    #[test]
    fn test_orderbook(bid in arb_bid(),mut book in arb_orderbook()) {
        println!("bad");
        let result = book.record_bid(bid);

        if bid.unit_price < book.price_floor {
            assert!(result.is_err() && result.unwrap() == OrderError::BidTooLow(book.price_floor));
            assert!(book.bids.last().unwrap().status == BidStatus::Unsuccessful(_));
        }

    }
}
