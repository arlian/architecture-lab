//! The Order aggregate, told entirely as a sequence of events.
//!
//! This is the one thing that's genuinely new versus every other lab: Orders
//! no longer has a `struct Order` that gets mutated and saved. It has a
//! `struct OrderState` that only ever exists as the *fold* of an event
//! history — `OrderState::fold(&events)`. There is no other way to know an
//! order's current status; you literally cannot ask for it without replaying
//! its history. That's event sourcing: the log is the source of truth, and
//! "current state" is a read, not a write.
//!
//! Business rules live in `service.rs`, which folds the history, checks the
//! resulting state against the requested command, and — if valid — asks the
//! event store to append one more event. `OrderState` itself only knows how
//! to fold; it has no opinion about what's *allowed*.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct OrderLineData {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

/// The full history of an order is a `Vec<OrderEvent>`. The event store keeps
/// these as plain Rust values in memory — they only get serialized when the
/// `history` endpoint hands the raw log out for inspection. What goes out
/// over NATS on every transition is a separate, smaller set of types in
/// events.rs. Conflating "the event I append to my own log" with "the event I
/// broadcast to the world" is a common event-sourcing mistake; keeping them
/// as two distinct types here is deliberate.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrderEvent {
    Placed {
        user_id: Uuid,
        lines: Vec<OrderLineData>,
        total_cents: u64,
    },
    Paid,
    Shipped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Placed,
    Paid,
    Shipped,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct OrderState {
    pub user_id: Uuid,
    pub lines: Vec<OrderLineData>,
    pub total_cents: u64,
    pub status: OrderStatus,
}

impl OrderState {
    /// Rebuild current state by replaying history from scratch. `None` means
    /// no `Placed` event has ever been recorded for this id — as far as this
    /// aggregate is concerned, the order doesn't exist.
    pub fn fold(events: &[OrderEvent]) -> Option<Self> {
        let mut state: Option<OrderState> = None;
        for event in events {
            state = Some(Self::apply(state, event));
        }
        state
    }

    fn apply(state: Option<OrderState>, event: &OrderEvent) -> OrderState {
        match event {
            OrderEvent::Placed {
                user_id,
                lines,
                total_cents,
            } => OrderState {
                user_id: *user_id,
                lines: lines.clone(),
                total_cents: *total_cents,
                status: OrderStatus::Placed,
            },
            OrderEvent::Paid => OrderState {
                status: OrderStatus::Paid,
                ..state.expect("Paid event applied before any Placed event")
            },
            OrderEvent::Shipped => OrderState {
                status: OrderStatus::Shipped,
                ..state.expect("Shipped event applied before any Placed event")
            },
            OrderEvent::Cancelled => OrderState {
                status: OrderStatus::Cancelled,
                ..state.expect("Cancelled event applied before any Placed event")
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_a_placed_order() {
        let user_id = Uuid::new_v4();
        let events = vec![OrderEvent::Placed {
            user_id,
            lines: vec![],
            total_cents: 500,
        }];
        let state = OrderState::fold(&events).unwrap();
        assert_eq!(state.status, OrderStatus::Placed);
        assert_eq!(state.total_cents, 500);
    }

    #[test]
    fn folds_a_full_lifecycle() {
        let events = vec![
            OrderEvent::Placed {
                user_id: Uuid::new_v4(),
                lines: vec![],
                total_cents: 500,
            },
            OrderEvent::Paid,
            OrderEvent::Shipped,
        ];
        let state = OrderState::fold(&events).unwrap();
        assert_eq!(state.status, OrderStatus::Shipped);
    }

    #[test]
    fn empty_history_is_no_order() {
        assert!(OrderState::fold(&[]).is_none());
    }
}
