//! Subscriptions that build `SharedOrderViews` from orders-command-service's
//! events. Subscriptions are established at startup (see `spawn`), before the
//! HTTP server starts accepting requests — a broken subject or unreachable
//! broker fails loudly at boot rather than as a mysteriously-empty read model
//! later.

use futures::StreamExt;

use crate::events::{OrderCancelled, OrderPaid, OrderPlaced, OrderShipped};
use crate::view::{OrderLineView, SharedOrderViews};

pub async fn spawn(nats: async_nats::Client, views: SharedOrderViews) {
    let placed_sub = nats
        .subscribe(OrderPlaced::SUBJECT)
        .await
        .expect("failed to subscribe to orders.placed");
    let paid_sub = nats
        .subscribe(OrderPaid::SUBJECT)
        .await
        .expect("failed to subscribe to orders.paid");
    let shipped_sub = nats
        .subscribe(OrderShipped::SUBJECT)
        .await
        .expect("failed to subscribe to orders.shipped");
    let cancelled_sub = nats
        .subscribe(OrderCancelled::SUBJECT)
        .await
        .expect("failed to subscribe to orders.cancelled");

    tokio::spawn(consume_placed(placed_sub, views.clone()));
    tokio::spawn(consume_paid(paid_sub, views.clone()));
    tokio::spawn(consume_shipped(shipped_sub, views.clone()));
    tokio::spawn(consume_cancelled(cancelled_sub, views));
}

async fn consume_placed(mut sub: async_nats::Subscriber, views: SharedOrderViews) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPlaced>(&msg.payload) {
            Ok(evt) => {
                tracing::debug!("projecting placed order {}", evt.id);
                let lines = evt
                    .lines
                    .into_iter()
                    .map(|l| OrderLineView {
                        product_id: l.product_id,
                        quantity: l.quantity,
                        unit_price_cents: l.unit_price_cents,
                    })
                    .collect();
                views
                    .insert_placed(evt.id, evt.user_id, lines, evt.total_cents)
                    .await;
            }
            Err(e) => tracing::warn!("bad OrderPlaced payload: {e}"),
        }
    }
}

async fn consume_paid(mut sub: async_nats::Subscriber, views: SharedOrderViews) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderPaid>(&msg.payload) {
            Ok(evt) => views.mark_paid(evt.id).await,
            Err(e) => tracing::warn!("bad OrderPaid payload: {e}"),
        }
    }
}

async fn consume_shipped(mut sub: async_nats::Subscriber, views: SharedOrderViews) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderShipped>(&msg.payload) {
            Ok(evt) => views.mark_shipped(evt.id).await,
            Err(e) => tracing::warn!("bad OrderShipped payload: {e}"),
        }
    }
}

async fn consume_cancelled(mut sub: async_nats::Subscriber, views: SharedOrderViews) {
    while let Some(msg) = sub.next().await {
        match serde_json::from_slice::<OrderCancelled>(&msg.payload) {
            Ok(evt) => views.mark_cancelled(evt.id).await,
            Err(e) => tracing::warn!("bad OrderCancelled payload: {e}"),
        }
    }
}
