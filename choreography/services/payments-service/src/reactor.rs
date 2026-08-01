//! Background tasks that feed facts from the bus into `PaymentsService`.
//!
//! The saga lab's version had one subscription. This has two, and neither of
//! them is a request to charge anything — one supplies the amount, the other
//! supplies the go-ahead, and the service decides when it has both.
//!
//! `spawn` establishes both subscriptions before returning, same
//! subscribe-before-serving-traffic rule as event-driven's projection.rs.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::UserId;
use crate::events::{OrderPlaced, StockReserved};
use crate::service::PaymentsService;

pub async fn spawn(nats: async_nats::Client, service: Arc<PaymentsService>) {
    let placed_sub = nats
        .subscribe(OrderPlaced::SUBJECT)
        .await
        .expect("failed to subscribe to orders.placed");
    let reserved_sub = nats
        .subscribe(StockReserved::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.stock_reserved");

    tokio::spawn(consume_placed(placed_sub, service.clone()));
    tokio::spawn(consume_reserved(reserved_sub, service));
}

async fn consume_placed(mut sub: async_nats::Subscriber, service: Arc<PaymentsService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPlaced>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("noting order {} at {}c", evt.id, evt.total_cents);
                service
                    .on_order_placed(evt.id, UserId(evt.user_id), evt.total_cents)
                    .await;
            }
            Err(e) => tracing::warn!("bad OrderPlaced payload: {e}"),
        }
    }
}

async fn consume_reserved(mut sub: async_nats::Subscriber, service: Arc<PaymentsService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReserved>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("stock reserved for order {}", evt.order_id);
                service.on_stock_reserved(evt.order_id).await;
            }
            Err(e) => tracing::warn!("bad StockReserved payload: {e}"),
        }
    }
}
