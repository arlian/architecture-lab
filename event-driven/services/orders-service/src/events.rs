//! Wire shapes for events Orders both consumes and produces.
//!
//! For the inbound events, Orders defines its **own** copy of the shape,
//! deserializing only the fields it actually needs — exactly like the old
//! `PriceView` in the microservices lab's clients.rs, which read only
//! `price_cents` out of Catalog's full product JSON. Nothing here is shared
//! with users-service or catalog-service at the type level, only at the level
//! of "we both agree this subject carries JSON shaped like this".

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Inbound: what Orders needs to know from Users and Catalog ---

#[derive(Debug, Deserialize)]
pub struct UserRegistered {
    pub id: Uuid,
}

impl UserRegistered {
    pub const SUBJECT: &'static str = "users.registered";
}

#[derive(Debug, Deserialize)]
pub struct ProductCreated {
    pub id: Uuid,
    pub price_cents: u64,
}

impl ProductCreated {
    pub const SUBJECT: &'static str = "catalog.product_created";
}

#[derive(Debug, Deserialize)]
pub struct ProductPriceChanged {
    pub id: Uuid,
    pub price_cents: u64,
}

impl ProductPriceChanged {
    pub const SUBJECT: &'static str = "catalog.product_price_changed";
}

// --- Outbound: what Orders tells the rest of the system ---

#[derive(Debug, Clone, Serialize)]
pub struct OrderPlacedLine {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lines: Vec<OrderPlacedLine>,
    pub total_cents: u64,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}
