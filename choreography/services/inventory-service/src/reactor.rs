//! Background tasks that turn facts other services published into calls on
//! `InventoryService`.
//!
//! The saga lab's version of this file was titled "turns incoming saga
//! commands into calls on `InventoryService`", and that word is the
//! difference. Nothing below is a command. These are two announcements
//! inventory-service chose to subscribe to, and the choosing is the design:
//!
//!   * `orders.placed` → reserve. Orders-service does not know this happens
//!     and does not need redeploying if we stop doing it.
//!   * `payments.declined` → release. This is the compensating action, and
//!     notice that **it is triggered by a peer, not by a supervisor**.
//!     Payments-service is not asking us to roll back; it does not know we
//!     reserved anything. It is stating a fact about a wallet, and we have
//!     independently concluded that the fact implies our stock should go
//!     back.
//!
//! Which means the rollback rule lives here, in the service that owns the
//! resource being rolled back. That is arguably where it belongs — and it is
//! also now impossible to read the rollback policy for the whole workflow in
//! one place, because there is no such place.
//!
//! `spawn` establishes both subscriptions before returning, same
//! subscribe-before-serving-traffic rule as event-driven's projection.rs.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::ProductId;
use crate::events::{OrderPlaced, PaymentDeclined};
use crate::service::InventoryService;

pub async fn spawn(nats: async_nats::Client, service: Arc<InventoryService>) {
    let placed_sub = nats
        .subscribe(OrderPlaced::SUBJECT)
        .await
        .expect("failed to subscribe to orders.placed");
    let declined_sub = nats
        .subscribe(PaymentDeclined::SUBJECT)
        .await
        .expect("failed to subscribe to payments.declined");

    tokio::spawn(consume_placed(placed_sub, service.clone()));
    tokio::spawn(consume_declined(declined_sub, service));
}

async fn consume_placed(mut sub: async_nats::Subscriber, service: Arc<InventoryService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPlaced>(&msg.payload) {
            Ok(evt) => {
                let lines = evt
                    .lines
                    .into_iter()
                    .map(|l| (ProductId(l.product_id), l.quantity))
                    .collect();
                tracing::debug!("order {} was placed; reserving its lines", evt.id);
                service.reserve(evt.id, lines).await;
            }
            Err(e) => tracing::warn!("bad OrderPlaced payload: {e}"),
        }
    }
}

async fn consume_declined(mut sub: async_nats::Subscriber, service: Arc<InventoryService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentDeclined>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!(
                    "payment for order {} was declined; releasing our reservation",
                    evt.order_id
                );
                service.release(evt.order_id).await;
            }
            Err(e) => tracing::warn!("bad PaymentDeclined payload: {e}"),
        }
    }
}
