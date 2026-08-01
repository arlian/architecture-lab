//! Notifications' private state: enough to turn a stream of other services'
//! facts into one email per order.
//!
//! ## This file should not have to exist
//!
//! In the saga lab, notifications-service was two files and about seventy
//! lines: subscribe to `orders.confirmed` and `orders.failed`, log a
//! sentence. It held no state at all, because the orchestrator had already
//! done the two hard parts — deciding when the order was over, and attaching
//! the customer's details to the announcement.
//!
//! With the orchestrator gone, both parts land here:
//!
//! * **The join.** Terminal facts (`payments.charged`,
//!   `inventory.stock_rejected`, `payments.declined`) carry an `order_id` and
//!   nothing else, because they are published by services that don't know
//!   who the customer is. So this service keeps its own copy of every order
//!   from `orders.placed` — the third such copy in the system, after
//!   payments-service's and orders-service's. Same unbounded-growth problem
//!   as payments-service's join buffer, same reason: no fact ever says "you
//!   may forget this order now."
//!
//! * **The definition of done.** Nothing on the bus says an order succeeded
//!   or failed. Those are conclusions, and this service draws its own.
//!
//! ## Where this service deliberately disagrees with orders-service
//!
//! On the payment-failure path, orders-service's tracker moves the order to
//! `compensating` when it sees `payments.declined`, and only calls it
//! `failed` once `inventory.stock_released` confirms the stock went back.
//!
//! This service treats `payments.declined` itself as terminal and emails the
//! customer right then. Which is defensible — the customer's order is dead
//! either way, and they do not care whether a warehouse counter has been
//! decremented yet — but it means that for a window of a few milliseconds,
//! the customer has been told their order failed while `GET /orders/:id`
//! still reports `compensating`.
//!
//! Neither service is wrong, and more to the point **nothing in the system
//! is in a position to say one of them is wrong**. In `saga/` this
//! disagreement was impossible to express: there was one `orders.failed`
//! event, published at one moment, by the one service that got to decide.
//! Choreography buys you services that can evolve independently by giving up
//! the guarantee that they agree.
//!
//! `notified` below is what keeps this honest at the only level it can be:
//! whatever this service concludes, it concludes once per order.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

/// What this service has decided happened to an order.
#[derive(Debug, Clone)]
pub enum Outcome {
    Confirmed,
    Failed(String),
}

/// The bits of an order needed to write the customer an email.
#[derive(Debug, Clone, Copy)]
pub struct PlacedOrder {
    pub user_id: Uuid,
    pub total_cents: u64,
}

#[derive(Default)]
struct State {
    placed: HashMap<Uuid, PlacedOrder>,
    /// Outcomes that arrived before the order details did.
    awaiting_details: HashMap<Uuid, Outcome>,
    /// Orders already emailed about, so a redelivered fact — or a second
    /// terminal fact for the same order — doesn't mail the customer twice.
    notified: HashSet<Uuid>,
}

#[derive(Default, Clone)]
pub struct Registry {
    state: Arc<RwLock<State>>,
}

impl Registry {
    /// Record `orders.placed`. Returns something to send only if a terminal
    /// fact for this order was already parked waiting on these details.
    pub async fn record_placed(
        &self,
        order_id: Uuid,
        order: PlacedOrder,
    ) -> Option<(PlacedOrder, Outcome)> {
        let mut state = self.state.write().await;
        state.placed.insert(order_id, order);
        let outcome = state.awaiting_details.remove(&order_id)?;
        if !state.notified.insert(order_id) {
            return None;
        }
        Some((order, outcome))
    }

    /// Record a terminal fact. Returns something to send if the order's
    /// details are already known; otherwise parks the outcome until
    /// `orders.placed` shows up.
    pub async fn record_outcome(
        &self,
        order_id: Uuid,
        outcome: Outcome,
    ) -> Option<(PlacedOrder, Outcome)> {
        let mut state = self.state.write().await;
        if state.notified.contains(&order_id) {
            return None;
        }
        // Bind before the let-else so the borrow of `state.placed` is
        // definitely released before the else block mutates `state`.
        let known = state.placed.get(&order_id).copied();
        let Some(order) = known else {
            // First terminal fact wins if several race us here; a later one
            // for the same order is dropped rather than overwriting.
            state.awaiting_details.entry(order_id).or_insert(outcome);
            return None;
        };
        state.notified.insert(order_id);
        Some((order, outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> PlacedOrder {
        PlacedOrder {
            user_id: Uuid::new_v4(),
            total_cents: 1299,
        }
    }

    #[tokio::test]
    async fn sends_once_the_details_and_outcome_are_both_known() {
        let reg = Registry::default();
        let id = Uuid::new_v4();

        assert!(reg.record_placed(id, order()).await.is_none());
        assert!(reg.record_outcome(id, Outcome::Confirmed).await.is_some());
    }

    /// The same race payments-service's join exists for: a terminal fact
    /// observed before `orders.placed`, because independent tasks drain the
    /// two subjects. Without parking, this customer never hears anything.
    #[tokio::test]
    async fn sends_when_the_outcome_is_observed_before_the_order() {
        let reg = Registry::default();
        let id = Uuid::new_v4();

        assert!(reg.record_outcome(id, Outcome::Confirmed).await.is_none());
        assert!(reg.record_placed(id, order()).await.is_some());
    }

    #[tokio::test]
    async fn never_emails_the_same_customer_twice() {
        let reg = Registry::default();
        let id = Uuid::new_v4();

        reg.record_placed(id, order()).await;
        assert!(reg.record_outcome(id, Outcome::Confirmed).await.is_some());
        assert!(reg.record_outcome(id, Outcome::Confirmed).await.is_none());
        assert!(reg
            .record_outcome(id, Outcome::Failed("late".into()))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn a_redelivered_placement_does_not_resend() {
        let reg = Registry::default();
        let id = Uuid::new_v4();

        reg.record_outcome(id, Outcome::Confirmed).await;
        assert!(reg.record_placed(id, order()).await.is_some());
        assert!(reg.record_placed(id, order()).await.is_none());
    }
}
