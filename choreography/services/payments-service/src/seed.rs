//! Background task that opens a starting wallet balance when Users announces
//! a new registration. Same "keep a local copy built from someone else's
//! events" idea as the event-driven lab's read models, just used to
//! initialize a resource instead of validate a fact.

use std::sync::Arc;

use futures::StreamExt;

use crate::domain::UserId;
use crate::events::UserRegistered;
use crate::service::PaymentsService;

/// Every new wallet opens with this balance — small on purpose, so the
/// "insufficient funds" failure (and the compensation that follows it) is
/// easy to trigger in the demo (see the README).
const STARTING_BALANCE_CENTS: u64 = 2000;

pub async fn spawn(nats: async_nats::Client, service: Arc<PaymentsService>) {
    let sub = nats
        .subscribe(UserRegistered::SUBJECT)
        .await
        .expect("failed to subscribe to users.registered");
    tokio::spawn(consume(sub, service));
}

async fn consume(mut sub: async_nats::Subscriber, service: Arc<PaymentsService>) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<UserRegistered>(&msg.payload) {
            Ok(evt) => {
                let user_id = UserId(evt.id);
                tracing::info!("opening wallet for user {user_id} with {STARTING_BALANCE_CENTS}c");
                service.open_wallet(user_id, STARTING_BALANCE_CENTS).await;
            }
            Err(e) => tracing::warn!("bad UserRegistered payload: {e}"),
        }
    }
}
