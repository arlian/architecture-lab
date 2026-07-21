//! # Notifications service — a pure event consumer, no HTTP surface at all.
//!
//! This is the payoff of choreography: orders-service never had to know this
//! service exists, import anything from it, or be redeployed to support it.
//! Adding a new subscriber to `orders.placed` was exactly one new file. Try
//! adding a `loyalty-service` or `inventory-service` the same way — that's the
//! learning exercise this service exists to demonstrate.

mod events;

use futures::StreamExt;

use events::OrderPlaced;

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

    let mut sub = nats
        .subscribe(OrderPlaced::SUBJECT)
        .await
        .expect("failed to subscribe to orders.placed");
    tracing::info!("notifications-service listening for {}", OrderPlaced::SUBJECT);

    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPlaced>(&msg.payload) {
            Ok(evt) => tracing::info!(
                "sending confirmation email for order {} (user {}, total {}c)",
                evt.id,
                evt.user_id,
                evt.total_cents
            ),
            Err(e) => tracing::warn!("bad OrderPlaced payload: {e}"),
        }
    }
}
