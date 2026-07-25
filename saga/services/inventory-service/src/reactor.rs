//! Background task that turns incoming saga commands into calls on
//! `InventoryService`. This is inventory-service's half of the saga
//! conversation: orders-service (the orchestrator) publishes a command,
//! this reactor executes it and the service publishes the reply.
//!
//! `spawn` establishes both subscriptions before returning, same
//! subscribe-before-serving-traffic rule as event-driven's projection.rs.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::ProductId;
use crate::events::{ReleaseStockRequested, ReserveStockRequested};
use crate::service::InventoryService;

pub async fn spawn(nats: async_nats::Client, service: Arc<InventoryService>) {
    let reserve_sub = nats
        .subscribe(ReserveStockRequested::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.reserve.requested");
    let release_sub = nats
        .subscribe(ReleaseStockRequested::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.release.requested");

    tokio::spawn(consume_reserve(reserve_sub, service.clone()));
    tokio::spawn(consume_release(release_sub, service));
}

async fn consume_reserve(mut sub: async_nats::Subscriber, service: Arc<InventoryService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ReserveStockRequested>(&msg.payload) {
            Ok(cmd) => {
                let lines = cmd
                    .lines
                    .into_iter()
                    .map(|l| (ProductId(l.product_id), l.quantity))
                    .collect();
                tracing::debug!("reserving stock for saga {}", cmd.saga_id);
                service.reserve(cmd.saga_id, lines).await;
            }
            Err(e) => tracing::warn!("bad ReserveStockRequested payload: {e}"),
        }
    }
}

async fn consume_release(mut sub: async_nats::Subscriber, service: Arc<InventoryService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ReleaseStockRequested>(&msg.payload) {
            Ok(cmd) => {
                tracing::debug!("releasing stock for saga {}", cmd.saga_id);
                service.release(cmd.saga_id).await;
            }
            Err(e) => tracing::warn!("bad ReleaseStockRequested payload: {e}"),
        }
    }
}
