//! # Notifications service — a pure event consumer, no HTTP surface at all.
//!
//! In event-driven this listened for `orders.placed`, published the moment
//! an order was accepted. Now that placing an order only starts a saga (see
//! orders-service/src/saga.rs), that moment isn't meaningful any more — an
//! order can still fail after being placed. So this service listens for the
//! saga's two *terminal* events instead: `orders.confirmed` and
//! `orders.failed`. orders-service still never had to know this service
//! exists, import anything from it, or be redeployed to support it — same
//! payoff as before, just anchored to a different point in the order's now
//! longer life cycle.

mod events;

use futures::StreamExt;

use events::{OrderConfirmed, OrderFailed};

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

    let confirmed_sub = nats
        .subscribe(OrderConfirmed::SUBJECT)
        .await
        .expect("failed to subscribe to orders.confirmed");
    let failed_sub = nats
        .subscribe(OrderFailed::SUBJECT)
        .await
        .expect("failed to subscribe to orders.failed");
    tracing::info!("notifications-service listening for orders.confirmed, orders.failed");

    tokio::join!(consume_confirmed(confirmed_sub), consume_failed(failed_sub));
}

async fn consume_confirmed(mut sub: async_nats::Subscriber) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderConfirmed>(&msg.payload) {
            Ok(evt) => tracing::info!(
                "sending confirmation email for order {} (user {}, total {}c)",
                evt.id,
                evt.user_id,
                evt.total_cents
            ),
            Err(e) => tracing::warn!("bad OrderConfirmed payload: {e}"),
        }
    }
}

async fn consume_failed(mut sub: async_nats::Subscriber) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderFailed>(&msg.payload) {
            Ok(evt) => tracing::info!(
                "sending sorry-your-order-failed email for order {} (user {}): {}",
                evt.id,
                evt.user_id,
                evt.reason
            ),
            Err(e) => tracing::warn!("bad OrderFailed payload: {e}"),
        }
    }
}
