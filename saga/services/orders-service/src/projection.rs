//! Background tasks that keep `ReadModel` in sync with events published by
//! Users and Catalog. This is the event-driven analogue of clients.rs in the
//! microservices lab: it's the one place Orders reaches out to its
//! neighbours. The difference is *when* — these subscriptions are made once
//! at startup and then just keep running, instead of a fresh HTTP call on
//! every `place()`.
//!
//! `spawn` establishes all three NATS subscriptions before returning, so a
//! broken subject or an unreachable broker fails loudly at startup rather
//! than silently inside a background task. Each subscription then runs its
//! own consume loop for the life of the process.

use futures::StreamExt;

use crate::events::{ProductCreated, ProductPriceChanged, UserRegistered};
use crate::read_model::ReadModel;

pub async fn spawn(nats: async_nats::Client, read_model: ReadModel) {
    let users_sub = nats
        .subscribe(UserRegistered::SUBJECT)
        .await
        .expect("failed to subscribe to users.registered");
    let products_created_sub = nats
        .subscribe(ProductCreated::SUBJECT)
        .await
        .expect("failed to subscribe to catalog.product_created");
    let price_changed_sub = nats
        .subscribe(ProductPriceChanged::SUBJECT)
        .await
        .expect("failed to subscribe to catalog.product_price_changed");

    tokio::spawn(consume_users(users_sub, read_model.clone()));
    tokio::spawn(consume_products_created(
        products_created_sub,
        read_model.clone(),
    ));
    tokio::spawn(consume_price_changed(price_changed_sub, read_model));
}

async fn consume_users(mut sub: async_nats::Subscriber, read_model: ReadModel) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<UserRegistered>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("projecting user {}", evt.id);
                read_model.record_user(evt.id).await;
            }
            Err(e) => tracing::warn!("bad UserRegistered payload: {e}"),
        }
    }
}

async fn consume_products_created(mut sub: async_nats::Subscriber, read_model: ReadModel) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ProductCreated>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("projecting product {} at {}c", evt.id, evt.price_cents);
                read_model.record_price(evt.id, evt.price_cents).await;
            }
            Err(e) => tracing::warn!("bad ProductCreated payload: {e}"),
        }
    }
}

async fn consume_price_changed(mut sub: async_nats::Subscriber, read_model: ReadModel) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<ProductPriceChanged>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("price update for product {}: {}c", evt.id, evt.price_cents);
                read_model.record_price(evt.id, evt.price_cents).await;
            }
            Err(e) => tracing::warn!("bad ProductPriceChanged payload: {e}"),
        }
    }
}
