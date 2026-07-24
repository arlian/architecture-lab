//! Events this service publishes — its entire public contract.
//! orders-command-service keeps its own copy of these shapes (see
//! orders-command-service/src/read_model.rs's inbound events); nothing is
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
