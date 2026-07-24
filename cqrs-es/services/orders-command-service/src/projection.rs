//! Background tasks that keep `ReadModel` (users/products, for command
//! validation) in sync with events from Users and Catalog. Identical role to
//! the event-driven lab's `orders-service/src/projection.rs`. Not to be
//! confused with orders-query-service's projection, which builds a read model
//! of *orders* from events *this* service publishes.

use futures::StreamExt;

use crate::read_model::ReadModel;

#[derive(serde::Deserialize)]
struct UserRegistered {
    id: uuid::Uuid,
}
const USER_REGISTERED_SUBJECT: &str = "users.registered";

#[derive(serde::Deserialize)]
struct ProductCreated {
    id: uuid::Uuid,
    price_cents: u64,
}
const PRODUCT_CREATED_SUBJECT: &str = "catalog.product_created";

#[derive(serde::Deserialize)]
struct ProductPriceChanged {
    id: uuid::Uuid,
    price_cents: u64,
}
const PRODUCT_PRICE_CHANGED_SUBJECT: &str = "catalog.product_price_changed";

pub async fn spawn(nats: async_nats::Client, read_model: ReadModel) {
    let users_sub = nats
        .subscribe(USER_REGISTERED_SUBJECT)
        .await
        .expect("failed to subscribe to users.registered");
    let products_created_sub = nats
        .subscribe(PRODUCT_CREATED_SUBJECT)
        .await
        .expect("failed to subscribe to catalog.product_created");
    let price_changed_sub = nats
        .subscribe(PRODUCT_PRICE_CHANGED_SUBJECT)
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
