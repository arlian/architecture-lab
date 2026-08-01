//! Events this service publishes — its entire public contract. Orders keeps
//! its own copy of these shapes (see orders-service/src/events.rs); nothing is
//! imported between services.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct ProductCreated {
    pub id: Uuid,
    pub name: String,
    pub price_cents: u64,
}

impl ProductCreated {
    pub const SUBJECT: &'static str = "catalog.product_created";
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductPriceChanged {
    pub id: Uuid,
    pub price_cents: u64,
}

impl ProductPriceChanged {
    pub const SUBJECT: &'static str = "catalog.product_price_changed";
}
