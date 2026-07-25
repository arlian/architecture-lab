//! Background task that turns incoming saga commands into calls on
//! `PaymentsService`. This is payments-service's half of the saga
//! conversation: orders-service (the orchestrator) publishes a command,
//! this reactor executes it and the service publishes the reply.
//!
//! `spawn` establishes the subscription before returning, same
//! subscribe-before-serving-traffic rule as event-driven's projection.rs.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::UserId;
use crate::events::ChargeRequested;
use crate::service::PaymentsService;

pub async fn spawn(nats: async_nats::Client, service: Arc<PaymentsService>) {
    let sub = nats
        .subscribe(ChargeRequested::SUBJECT)
        .await
        .expect("failed to subscribe to payments.charge.requested");
    tokio::spawn(consume(sub, service));
}

async fn consume(mut sub: async_nats::Subscriber, service: Arc<PaymentsService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ChargeRequested>(&msg.payload) {
            Ok(cmd) => {
                tracing::debug!("charging {}c for saga {}", cmd.amount_cents, cmd.saga_id);
                service
                    .charge(cmd.saga_id, UserId(cmd.user_id), cmd.amount_cents)
                    .await;
            }
            Err(e) => tracing::warn!("bad ChargeRequested payload: {e}"),
        }
    }
}
