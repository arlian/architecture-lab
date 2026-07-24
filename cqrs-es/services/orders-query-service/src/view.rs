//! The query side's entire reason to exist: a read-optimized projection of
//! orders, built by folding the same events orders-command-service publishes.
//! Nothing here is "the" state of an order — it's *a* state, shaped for the
//! queries this service wants to answer cheaply (list all, list by user, get
//! one). A different consumer of the same events could build a completely
//! different shape (notifications-service does exactly that, with a
//! projection so small it never even stores anything).
//!
//! If this in-memory map were lost (a restart), it could in principle be
//! rebuilt from scratch by replaying orders-command-service's full history —
//! that's the standard CQRS/ES promise. This lab doesn't actually implement
//! that replay path (plain NATS core has no history to replay), so today a
//! restart genuinely loses the projection. See the README's exercises.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Placed,
    Paid,
    Shipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderLineView {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderView {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lines: Vec<OrderLineView>,
    pub total_cents: u64,
    pub status: OrderStatus,
}

#[derive(Default)]
pub struct OrderViews {
    by_id: RwLock<HashMap<Uuid, OrderView>>,
}

#[derive(Default, Clone)]
pub struct SharedOrderViews(Arc<OrderViews>);

impl SharedOrderViews {
    pub async fn insert_placed(
        &self,
        id: Uuid,
        user_id: Uuid,
        lines: Vec<OrderLineView>,
        total_cents: u64,
    ) {
        self.0.by_id.write().await.insert(
            id,
            OrderView {
                id,
                user_id,
                lines,
                total_cents,
                status: OrderStatus::Placed,
            },
        );
    }

    async fn set_status(&self, id: Uuid, status: OrderStatus) {
        if let Some(view) = self.0.by_id.write().await.get_mut(&id) {
            view.status = status;
        } else {
            // The status-change event arrived before the Placed event we'd
            // need to have a view to update — a real symptom of eventual
            // consistency / out-of-order delivery. We just drop it; see the
            // README for why NATS core gives no ordering guarantee across
            // subjects and what a fix would look like.
            tracing::warn!("received a status update for unknown order {id}");
        }
    }

    pub async fn mark_paid(&self, id: Uuid) {
        self.set_status(id, OrderStatus::Paid).await;
    }

    pub async fn mark_shipped(&self, id: Uuid) {
        self.set_status(id, OrderStatus::Shipped).await;
    }

    pub async fn mark_cancelled(&self, id: Uuid) {
        self.set_status(id, OrderStatus::Cancelled).await;
    }

    pub async fn get(&self, id: Uuid) -> Option<OrderView> {
        self.0.by_id.read().await.get(&id).cloned()
    }

    /// All orders, optionally filtered by user — a query shape the write side
    /// never needed and never offers. That asymmetry is the whole argument
    /// for CQRS: the read side is free to grow query capabilities the write
    /// side has no reason to carry.
    pub async fn list(&self, user_id: Option<Uuid>) -> Vec<OrderView> {
        self.0
            .by_id
            .read()
            .await
            .values()
            .filter(|v| match user_id {
                Some(uid) => v.user_id == uid,
                None => true,
            })
            .cloned()
            .collect()
    }
}
