//! The saga orchestrator reactor. This is the new thing versus event-driven:
//! orders-service drives the *whole* multi-step workflow from here instead
//! of each service reacting independently to whatever it happens to hear.
//! Every handler below reacts to one participant reply, decides the next
//! step, and either publishes the next command or a terminal
//! `orders.confirmed`/`orders.failed`.
//!
//! Each transition is guarded by `OrderRepository::transition`'s
//! expected-status check, so a redelivered or out-of-order reply (there's no
//! at-most-once guarantee on plain NATS core) can't re-apply a step that
//! already happened — the order simply stays wherever it already is.
//!
//! `spawn` establishes every subscription before returning, same
//! subscribe-before-serving-traffic rule as projection.rs.

use std::sync::Arc;

use futures::StreamExt;
use serde::Serialize;

use crate::bus::EventBus;
use crate::domain::{OrderId, OrderStatus};
use crate::events::{
    ChargeRequested, OrderConfirmed, OrderFailed, PaymentCharged, PaymentChargeFailed,
    ReleaseStockRequested, StockReleased, StockReserveFailed, StockReserved,
};
use crate::repository::OrderRepository;

pub async fn spawn(
    nats: async_nats::Client,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    let reserve_succeeded = nats
        .subscribe(StockReserved::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.reserve.succeeded");
    let reserve_failed = nats
        .subscribe(StockReserveFailed::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.reserve.failed");
    let charge_succeeded = nats
        .subscribe(PaymentCharged::SUBJECT)
        .await
        .expect("failed to subscribe to payments.charge.succeeded");
    let charge_failed = nats
        .subscribe(PaymentChargeFailed::SUBJECT)
        .await
        .expect("failed to subscribe to payments.charge.failed");
    let release_succeeded = nats
        .subscribe(StockReleased::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.release.succeeded");

    tokio::spawn(on_reserve_succeeded(reserve_succeeded, repo.clone(), events.clone()));
    tokio::spawn(on_reserve_failed(reserve_failed, repo.clone(), events.clone()));
    tokio::spawn(on_charge_succeeded(charge_succeeded, repo.clone(), events.clone()));
    tokio::spawn(on_charge_failed(charge_failed, repo.clone(), events.clone()));
    tokio::spawn(on_release_succeeded(release_succeeded, repo, events));
}

async fn publish<T: Serialize>(events: &Arc<dyn EventBus>, subject: &'static str, event: &T) {
    let payload = serde_json::to_vec(event).expect("event is serializable");
    if let Err(e) = events.publish(subject, payload).await {
        tracing::warn!("failed to publish {subject}: {e}");
    }
}

/// Step 1 succeeded: stock reserved. Ask payments-service to charge the
/// order's total — step 2.
async fn on_reserve_succeeded(
    mut sub: async_nats::Subscriber,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReserved>(&msg.payload) {
            Ok(reply) => {
                let saga_id = reply.saga_id;
                let Some(order) = repo
                    .transition(OrderId(saga_id), OrderStatus::Pending, OrderStatus::AwaitingPayment, None)
                    .await
                else {
                    tracing::debug!("ignoring reserve.succeeded for saga {saga_id}: not Pending");
                    continue;
                };
                tracing::info!(
                    "stock reserved for order {saga_id}; requesting payment of {}c",
                    order.total_cents
                );
                publish(
                    &events,
                    ChargeRequested::SUBJECT,
                    &ChargeRequested {
                        saga_id,
                        user_id: order.user_id.0,
                        amount_cents: order.total_cents,
                    },
                )
                .await;
            }
            Err(e) => tracing::warn!("bad StockReserved payload: {e}"),
        }
    }
}

/// Step 1 failed: no stock to reserve. Nothing was reserved, so there's
/// nothing to compensate — the order fails immediately.
async fn on_reserve_failed(
    mut sub: async_nats::Subscriber,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReserveFailed>(&msg.payload) {
            Ok(reply) => {
                let saga_id = reply.saga_id;
                let Some(order) = repo
                    .transition(
                        OrderId(saga_id),
                        OrderStatus::Pending,
                        OrderStatus::Failed,
                        Some(reply.reason.clone()),
                    )
                    .await
                else {
                    tracing::debug!("ignoring reserve.failed for saga {saga_id}: not Pending");
                    continue;
                };
                tracing::info!("order {saga_id} failed: {}", reply.reason);
                publish(
                    &events,
                    OrderFailed::SUBJECT,
                    &OrderFailed {
                        id: saga_id,
                        user_id: order.user_id.0,
                        reason: reply.reason,
                    },
                )
                .await;
            }
            Err(e) => tracing::warn!("bad StockReserveFailed payload: {e}"),
        }
    }
}

/// Step 2 succeeded: payment charged. The saga is done — confirm the order.
async fn on_charge_succeeded(
    mut sub: async_nats::Subscriber,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentCharged>(&msg.payload) {
            Ok(reply) => {
                let saga_id = reply.saga_id;
                let Some(order) = repo
                    .transition(OrderId(saga_id), OrderStatus::AwaitingPayment, OrderStatus::Confirmed, None)
                    .await
                else {
                    tracing::debug!("ignoring charge.succeeded for saga {saga_id}: not AwaitingPayment");
                    continue;
                };
                tracing::info!("order {saga_id} confirmed");
                publish(
                    &events,
                    OrderConfirmed::SUBJECT,
                    &OrderConfirmed {
                        id: saga_id,
                        user_id: order.user_id.0,
                        total_cents: order.total_cents,
                    },
                )
                .await;
            }
            Err(e) => tracing::warn!("bad PaymentCharged payload: {e}"),
        }
    }
}

/// Step 2 failed: insufficient funds. Stock *was* reserved in step 1, so this
/// is where the saga needs its compensating action — release it.
async fn on_charge_failed(
    mut sub: async_nats::Subscriber,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentChargeFailed>(&msg.payload) {
            Ok(reply) => {
                let saga_id = reply.saga_id;
                let transitioned = repo
                    .transition(
                        OrderId(saga_id),
                        OrderStatus::AwaitingPayment,
                        OrderStatus::Compensating,
                        Some(reply.reason.clone()),
                    )
                    .await;
                if transitioned.is_none() {
                    tracing::debug!("ignoring charge.failed for saga {saga_id}: not AwaitingPayment");
                    continue;
                }
                tracing::info!(
                    "payment failed for order {saga_id}: {}; releasing reserved stock",
                    reply.reason
                );
                publish(
                    &events,
                    ReleaseStockRequested::SUBJECT,
                    &ReleaseStockRequested { saga_id },
                )
                .await;
            }
            Err(e) => tracing::warn!("bad PaymentChargeFailed payload: {e}"),
        }
    }
}

/// Compensation complete: the stock reserved in step 1 has been released.
/// The order is now finally, terminally failed.
async fn on_release_succeeded(
    mut sub: async_nats::Subscriber,
    repo: Arc<dyn OrderRepository>,
    events: Arc<dyn EventBus>,
) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockReleased>(&msg.payload) {
            Ok(reply) => {
                let saga_id = reply.saga_id;
                let Some(order) = repo
                    .transition(OrderId(saga_id), OrderStatus::Compensating, OrderStatus::Failed, None)
                    .await
                else {
                    tracing::debug!("ignoring release.succeeded for saga {saga_id}: not Compensating");
                    continue;
                };
                let reason = order
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| "payment declined".into());
                tracing::info!("order {saga_id} failed after compensation: {reason}");
                publish(
                    &events,
                    OrderFailed::SUBJECT,
                    &OrderFailed {
                        id: saga_id,
                        user_id: order.user_id.0,
                        reason,
                    },
                )
                .await;
            }
            Err(e) => tracing::warn!("bad StockReleased payload: {e}"),
        }
    }
}
