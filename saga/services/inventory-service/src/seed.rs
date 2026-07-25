//! Background task that seeds a starting stock count when Catalog announces
//! a new product. Same "keep a local copy built from someone else's events"
//! idea as the event-driven lab's read models, just used to initialize a
//! resource instead of validate a fact.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::ProductId;
use crate::events::ProductCreated;
use crate::service::InventoryService;

/// Every new product starts with this many units in stock — small on
/// purpose, so the "insufficient stock" saga failure path is easy to trigger
/// in the demo (see the README).
const STARTING_STOCK_UNITS: u32 = 5;

pub async fn spawn(nats: async_nats::Client, service: Arc<InventoryService>) {
    let sub = nats
        .subscribe(ProductCreated::SUBJECT)
        .await
        .expect("failed to subscribe to catalog.product_created");
    tokio::spawn(consume(sub, service));
}

async fn consume(mut sub: async_nats::Subscriber, service: Arc<InventoryService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ProductCreated>(&msg.payload) {
            Ok(evt) => {
                let product_id = ProductId(evt.id);
                tracing::info!(
                    "seeding {STARTING_STOCK_UNITS} units of stock for product {product_id}"
                );
                service.seed_product(product_id, STARTING_STOCK_UNITS).await;
            }
            Err(e) => tracing::warn!("bad ProductCreated payload: {e}"),
        }
    }
}
