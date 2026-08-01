//! This file is what `saga/services/orders-service/src/saga.rs` turned into
//! when the orchestrator was deleted. **Read the two side by side — that
//! diff is the whole lab.**
//!
//! They subscribe to nearly the same subjects and run nearly the same state
//! machine. The difference is what happens at the end of each handler. In
//! `saga.rs`, every handler ended by publishing the next command: it decided
//! what the system should do next, and the system did it. Here, every
//! handler ends at the `transition` call. Nothing is published. Nothing is
//! decided.
//!
//! That's because the participants are not waiting on us. When
//! `inventory.stock_reserved` lands in this process, payments-service has
//! already received the identical broadcast and is already charging the
//! wallet — possibly before this handler even runs. Orders-service is
//! reading a play-by-play of a workflow it does not control, and the
//! `status` field it maintains is a *report*, not a *decision*.
//!
//! Two consequences worth sitting with:
//!
//! 1. **The workflow survives this service.** Kill orders-service right
//!    after a `POST /orders` and the order still gets reserved, charged and
//!    (on failure) compensated — inventory and payments never needed us. In
//!    the saga lab the same kill halts the workflow at whatever step it had
//!    reached, forever. Losing the orchestrator lost the workflow; losing
//!    the tracker only loses the *view* of it.
//! 2. **Nothing here is authoritative.** If this file has a bug, or this
//!    service is down for the window in which a fact is broadcast, the
//!    status is simply wrong and nothing in the system disagrees with it.
//!    There is no other copy to reconcile against. In `saga/`,
//!    `OrderStatus` *was* the truth, because it was the thing driving the
//!    next step.
//!
//! `spawn` establishes every subscription before returning, same
//! subscribe-before-serving-traffic rule as projection.rs.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::{OrderId, OrderStatus};
use crate::events::{
    PaymentCharged, PaymentDeclined, StockRejected, StockReleased, StockReserved,
};
use crate::repository::OrderRepository;

pub async fn spawn(nats: async_nats::Client, repo: Arc<dyn OrderRepository>) {
    let reserved = nats
        .subscribe(StockReserved::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.stock_reserved");
    let rejected = nats
        .subscribe(StockRejected::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.stock_rejected");
    let charged = nats
        .subscribe(PaymentCharged::SUBJECT)
        .await
        .expect("failed to subscribe to payments.charged");
    let declined = nats
        .subscribe(PaymentDeclined::SUBJECT)
        .await
        .expect("failed to subscribe to payments.declined");
    let released = nats
        .subscribe(StockReleased::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.stock_released");

    tokio::spawn(on_stock_reserved(reserved, repo.clone()));
    tokio::spawn(on_stock_rejected(rejected, repo.clone()));
    tokio::spawn(on_payment_charged(charged, repo.clone()));
    tokio::spawn(on_payment_declined(declined, repo.clone()));
    tokio::spawn(on_stock_released(released, repo));
}

/// Heard: inventory reserved stock. By now payments-service has the same
/// message and is charging the wallet.
async fn on_stock_reserved(mut sub: async_nats::Subscriber, repo: Arc<dyn OrderRepository>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReserved>(&msg.payload) {
            Ok(evt) => {
                let id = evt.order_id;
                if repo
                    .transition(
                        OrderId(id),
                        OrderStatus::Pending,
                        OrderStatus::AwaitingPayment,
                        None,
                    )
                    .await
                    .is_none()
                {
                    tracing::debug!("ignoring stock_reserved for order {id}: not Pending");
                    continue;
                }
                tracing::info!("order {id}: stock reserved (someone is charging it now)");
            }
            Err(e) => tracing::warn!("bad StockReserved payload: {e}"),
        }
    }
}

/// Heard: inventory could not reserve stock. Nothing was reserved, so no
/// compensation is coming — this is terminal.
async fn on_stock_rejected(mut sub: async_nats::Subscriber, repo: Arc<dyn OrderRepository>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockRejected>(&msg.payload) {
            Ok(evt) => {
                let id = evt.order_id;
                if repo
                    .transition(
                        OrderId(id),
                        OrderStatus::Pending,
                        OrderStatus::Failed,
                        Some(evt.reason.clone()),
                    )
                    .await
                    .is_none()
                {
                    tracing::debug!("ignoring stock_rejected for order {id}: not Pending");
                    continue;
                }
                tracing::info!("order {id}: failed, {}", evt.reason);
            }
            Err(e) => tracing::warn!("bad StockRejected payload: {e}"),
        }
    }
}

/// Heard: the wallet was charged. As far as we can tell, this order is done.
///
/// "As far as we can tell" is doing real work in that sentence. No service
/// in this system ever declares an order complete — completion is an
/// inference each interested service draws for itself, and
/// notifications-service draws the same one independently. See its main.rs.
async fn on_payment_charged(mut sub: async_nats::Subscriber, repo: Arc<dyn OrderRepository>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentCharged>(&msg.payload) {
            Ok(evt) => {
                let id = evt.order_id;
                if repo
                    .transition(
                        OrderId(id),
                        OrderStatus::AwaitingPayment,
                        OrderStatus::Confirmed,
                        None,
                    )
                    .await
                    .is_none()
                {
                    tracing::debug!("ignoring payments.charged for order {id}: not AwaitingPayment");
                    continue;
                }
                tracing::info!("order {id}: confirmed");
            }
            Err(e) => tracing::warn!("bad PaymentCharged payload: {e}"),
        }
    }
}

/// Heard: payment was declined. In the saga lab this handler was where the
/// compensating `ReleaseStockRequested` command got published — the
/// orchestrator had to know that a failed charge implies releasing stock,
/// and had to know who to ask.
///
/// Here we publish nothing. inventory-service subscribes to
/// `payments.declined` itself and is already releasing. All we do is note
/// that the order is in the middle of being unwound.
async fn on_payment_declined(mut sub: async_nats::Subscriber, repo: Arc<dyn OrderRepository>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentDeclined>(&msg.payload) {
            Ok(evt) => {
                let id = evt.order_id;
                if repo
                    .transition(
                        OrderId(id),
                        OrderStatus::AwaitingPayment,
                        OrderStatus::Compensating,
                        Some(evt.reason.clone()),
                    )
                    .await
                    .is_none()
                {
                    tracing::debug!("ignoring payments.declined for order {id}: not AwaitingPayment");
                    continue;
                }
                tracing::info!(
                    "order {id}: payment declined ({}); someone should be releasing the stock",
                    evt.reason
                );
            }
            Err(e) => tracing::warn!("bad PaymentDeclined payload: {e}"),
        }
    }
}

/// Heard: the reserved stock went back. The unwind is complete, so the order
/// is now terminally failed.
async fn on_stock_released(mut sub: async_nats::Subscriber, repo: Arc<dyn OrderRepository>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReleased>(&msg.payload) {
            Ok(evt) => {
                let id = evt.order_id;
                let Some(order) = repo
                    .transition(
                        OrderId(id),
                        OrderStatus::Compensating,
                        OrderStatus::Failed,
                        None,
                    )
                    .await
                else {
                    tracing::debug!("ignoring stock_released for order {id}: not Compensating");
                    continue;
                };
                let reason = order
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| "payment declined".into());
                tracing::info!("order {id}: failed after compensation, {reason}");
            }
            Err(e) => tracing::warn!("bad StockReleased payload: {e}"),
        }
    }
}
