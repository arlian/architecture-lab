//! # Notifications service — still a pure event consumer, but no longer a
//! simple one.
//!
//! Across the earlier labs this service kept getting easier to point at as
//! the payoff of decoupling: nobody knows it exists, it just listens and
//! reacts. That is still true here — no service in this workspace mentions
//! notifications — but the cost side of the ledger is finally visible.
//!
//! In `event-driven/` it listened for one subject. In `saga/` it listened
//! for two, `orders.confirmed` and `orders.failed`, both handed to it
//! pre-interpreted by the orchestrator. Here it listens for four, none of
//! which is about orders being finished, and has to work out what they mean
//! (registry.rs).
//!
//! Adding a consumer stayed cheap. Understanding the workflow well enough to
//! *write* one got much more expensive — you now have to know which
//! combination of three other services' internal facts adds up to "the
//! customer's order is done", and there is no file anywhere that tells you.

mod events;
mod registry;

use futures::StreamExt;

use events::{OrderPlaced, PaymentCharged, PaymentDeclined, StockRejected};
use registry::{Outcome, PlacedOrder, Registry};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,notifications_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let placed = nats
        .subscribe(OrderPlaced::SUBJECT)
        .await
        .expect("failed to subscribe to orders.placed");
    let charged = nats
        .subscribe(PaymentCharged::SUBJECT)
        .await
        .expect("failed to subscribe to payments.charged");
    let rejected = nats
        .subscribe(StockRejected::SUBJECT)
        .await
        .expect("failed to subscribe to inventory.stock_rejected");
    let declined = nats
        .subscribe(PaymentDeclined::SUBJECT)
        .await
        .expect("failed to subscribe to payments.declined");
    tracing::info!(
        "notifications-service listening for orders.placed, payments.charged, inventory.stock_rejected, payments.declined"
    );

    let registry = Registry::default();

    tokio::join!(
        consume_placed(placed, registry.clone()),
        consume_charged(charged, registry.clone()),
        consume_rejected(rejected, registry.clone()),
        consume_declined(declined, registry),
    );
}

/// Not a notification trigger — just the only place this service can learn
/// who to write to.
async fn consume_placed(mut sub: async_nats::Subscriber, registry: Registry) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPlaced>(&msg.payload) {
            Ok(evt) => {
                let order = PlacedOrder {
                    user_id: evt.user_id,
                    total_cents: evt.total_cents,
                };
                if let Some((order, outcome)) = registry.record_placed(evt.id, order).await {
                    send(evt.id, order, outcome);
                }
            }
            Err(e) => tracing::warn!("bad OrderPlaced payload: {e}"),
        }
    }
}

async fn consume_charged(mut sub: async_nats::Subscriber, registry: Registry) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentCharged>(&msg.payload) {
            Ok(evt) => {
                if let Some((order, outcome)) =
                    registry.record_outcome(evt.order_id, Outcome::Confirmed).await
                {
                    send(evt.order_id, order, outcome);
                }
            }
            Err(e) => tracing::warn!("bad PaymentCharged payload: {e}"),
        }
    }
}

async fn consume_rejected(mut sub: async_nats::Subscriber, registry: Registry) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<StockRejected>(&msg.payload) {
            Ok(evt) => {
                if let Some((order, outcome)) = registry
                    .record_outcome(evt.order_id, Outcome::Failed(evt.reason))
                    .await
                {
                    send(evt.order_id, order, outcome);
                }
            }
            Err(e) => tracing::warn!("bad StockRejected payload: {e}"),
        }
    }
}

/// Note that this service does *not* wait for `inventory.stock_released`
/// before telling the customer. orders-service does. See registry.rs for why
/// that difference is deliberate, and why nothing can adjudicate it.
async fn consume_declined(mut sub: async_nats::Subscriber, registry: Registry) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<PaymentDeclined>(&msg.payload) {
            Ok(evt) => {
                if let Some((order, outcome)) = registry
                    .record_outcome(evt.order_id, Outcome::Failed(evt.reason))
                    .await
                {
                    send(evt.order_id, order, outcome);
                }
            }
            Err(e) => tracing::warn!("bad PaymentDeclined payload: {e}"),
        }
    }
}

fn send(order_id: uuid::Uuid, order: PlacedOrder, outcome: Outcome) {
    match outcome {
        Outcome::Confirmed => tracing::info!(
            "sending confirmation email for order {order_id} (user {}, total {}c)",
            order.user_id,
            order.total_cents
        ),
        Outcome::Failed(reason) => tracing::info!(
            "sending sorry-your-order-failed email for order {order_id} (user {}): {reason}",
            order.user_id
        ),
    }
}
