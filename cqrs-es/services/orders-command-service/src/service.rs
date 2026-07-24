//! Order commands. Every handler follows the same shape: load history, fold
//! it into current state, check the requested transition is legal from that
//! state, append one new event, publish it, return the resulting view.
//!
//! Compare `place` here to the event-driven lab's `OrderService::place`: the
//! validation against Users/Catalog (via `read_model`) is unchanged. What's
//! new is everything after that — instead of building an `Order` struct and
//! saving it, we build one `OrderEvent::Placed` and append it. The "order"
//! doesn't exist as a row anywhere; it exists as that one event, and later as
//! that event plus whatever comes after it.

use std::sync::Arc;

use uuid::Uuid;

use crate::aggregate::{OrderEvent, OrderLineData, OrderState, OrderStatus};
use crate::bus::EventBus;
use crate::domain::OrderId;
use crate::error::AppError;
use crate::event_store::EventStore;
use crate::events::{OrderCancelled, OrderPaid, OrderPlaced, OrderPlacedLine, OrderShipped};
use crate::read_model::ReadModel;

pub struct PlaceOrderLine {
    pub product_id: Uuid,
    pub quantity: u32,
}

pub struct PlaceOrder {
    pub user_id: Uuid,
    pub lines: Vec<PlaceOrderLine>,
}

/// The HTTP-facing view of an order: its id plus the folded state. Both
/// command responses and the raw `history` endpoint hand this shape (or the
/// raw events, for history) back to callers.
pub struct OrderView {
    pub id: OrderId,
    pub state: OrderState,
}

pub struct OrderCommandService {
    store: EventStore,
    read_model: ReadModel,
    events: Arc<dyn EventBus>,
}

impl OrderCommandService {
    pub fn new(store: EventStore, read_model: ReadModel, events: Arc<dyn EventBus>) -> Self {
        Self {
            store,
            read_model,
            events,
        }
    }

    pub async fn place(&self, cmd: PlaceOrder) -> Result<OrderView, AppError> {
        if cmd.lines.is_empty() {
            return Err(AppError::Validation("an order needs at least one line".into()));
        }
        if !self.read_model.user_exists(cmd.user_id).await {
            return Err(AppError::Validation(format!(
                "user {} does not exist",
                cmd.user_id
            )));
        }

        let mut lines = Vec::with_capacity(cmd.lines.len());
        let mut total_cents: u64 = 0;
        for line in cmd.lines {
            if line.quantity == 0 {
                return Err(AppError::Validation("quantity must be at least 1".into()));
            }
            let unit_price_cents = self
                .read_model
                .price_of(line.product_id)
                .await
                .ok_or_else(|| {
                    AppError::Validation(format!("product {} does not exist", line.product_id))
                })?;

            total_cents += unit_price_cents * line.quantity as u64;
            lines.push(OrderLineData {
                product_id: line.product_id,
                quantity: line.quantity,
                unit_price_cents,
            });
        }

        let id = OrderId::new();
        let event = OrderEvent::Placed {
            user_id: cmd.user_id,
            lines: lines.clone(),
            total_cents,
        };
        self.store.append(id, event.clone()).await;

        let wire_event = OrderPlaced {
            id: id.0,
            user_id: cmd.user_id,
            lines: lines
                .iter()
                .map(|l| OrderPlacedLine {
                    product_id: l.product_id,
                    quantity: l.quantity,
                    unit_price_cents: l.unit_price_cents,
                })
                .collect(),
            total_cents,
        };
        self.publish(OrderPlaced::SUBJECT, &wire_event, id).await;

        Ok(OrderView {
            id,
            state: OrderState::fold(&[event]).expect("just appended a Placed event"),
        })
    }

    pub async fn pay(&self, id: OrderId) -> Result<OrderView, AppError> {
        let mut history = self.store.load(id).await;
        let state = OrderState::fold(&history).ok_or_else(|| AppError::NotFound(format!("order {id}")))?;
        if state.status != OrderStatus::Placed {
            return Err(AppError::Validation(format!(
                "order {id} cannot be paid for from status {:?}",
                state.status
            )));
        }

        self.store.append(id, OrderEvent::Paid).await;
        history.push(OrderEvent::Paid);
        self.publish(OrderPaid::SUBJECT, &OrderPaid { id: id.0 }, id).await;

        Ok(OrderView {
            id,
            state: OrderState::fold(&history).expect("history is non-empty"),
        })
    }

    pub async fn ship(&self, id: OrderId) -> Result<OrderView, AppError> {
        let mut history = self.store.load(id).await;
        let state = OrderState::fold(&history).ok_or_else(|| AppError::NotFound(format!("order {id}")))?;
        if state.status != OrderStatus::Paid {
            return Err(AppError::Validation(format!(
                "order {id} cannot be shipped from status {:?}",
                state.status
            )));
        }

        self.store.append(id, OrderEvent::Shipped).await;
        history.push(OrderEvent::Shipped);
        self.publish(OrderShipped::SUBJECT, &OrderShipped { id: id.0 }, id)
            .await;

        Ok(OrderView {
            id,
            state: OrderState::fold(&history).expect("history is non-empty"),
        })
    }

    pub async fn cancel(&self, id: OrderId) -> Result<OrderView, AppError> {
        let mut history = self.store.load(id).await;
        let state = OrderState::fold(&history).ok_or_else(|| AppError::NotFound(format!("order {id}")))?;
        if !matches!(state.status, OrderStatus::Placed | OrderStatus::Paid) {
            return Err(AppError::Validation(format!(
                "order {id} cannot be cancelled from status {:?}",
                state.status
            )));
        }

        self.store.append(id, OrderEvent::Cancelled).await;
        history.push(OrderEvent::Cancelled);
        self.publish(OrderCancelled::SUBJECT, &OrderCancelled { id: id.0 }, id)
            .await;

        Ok(OrderView {
            id,
            state: OrderState::fold(&history).expect("history is non-empty"),
        })
    }

    /// The raw event history for one order — the audit trail an event-sourced
    /// aggregate gets for free. Nothing like this exists in the other labs:
    /// there is no `Order` row to diff against a changelog, there is only
    /// this sequence of events, and this endpoint is just handing it out.
    pub async fn history(&self, id: OrderId) -> Result<Vec<OrderEvent>, AppError> {
        let history = self.store.load(id).await;
        if history.is_empty() {
            return Err(AppError::NotFound(format!("order {id}")));
        }
        Ok(history)
    }

    async fn publish<E: serde::Serialize>(&self, subject: &'static str, event: &E, id: OrderId) {
        let payload = serde_json::to_vec(event).expect("event is serializable");
        if let Err(e) = self.events.publish(subject, payload).await {
            tracing::warn!("failed to publish {subject} for order {id}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct FakeBus {
        published: Mutex<Vec<&'static str>>,
    }

    #[async_trait::async_trait]
    impl EventBus for FakeBus {
        async fn publish(&self, subject: &'static str, _payload: Vec<u8>) -> Result<(), AppError> {
            self.published.lock().await.push(subject);
            Ok(())
        }
    }

    async fn service() -> (OrderCommandService, ReadModel, Arc<FakeBus>) {
        let read_model = ReadModel::default();
        let bus = Arc::new(FakeBus::default());
        let svc = OrderCommandService::new(EventStore::default(), read_model.clone(), bus.clone());
        (svc, read_model, bus)
    }

    #[tokio::test]
    async fn places_an_order() {
        let (svc, read_model, bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 250).await;

        let view = svc
            .place(PlaceOrder {
                user_id,
                lines: vec![PlaceOrderLine {
                    product_id,
                    quantity: 2,
                }],
            })
            .await
            .unwrap();

        assert_eq!(view.state.total_cents, 500);
        assert_eq!(view.state.status, OrderStatus::Placed);
        assert_eq!(*bus.published.lock().await, vec![OrderPlaced::SUBJECT]);
    }

    #[tokio::test]
    async fn full_lifecycle_pay_then_ship() {
        let (svc, read_model, bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 100).await;

        let placed = svc
            .place(PlaceOrder {
                user_id,
                lines: vec![PlaceOrderLine {
                    product_id,
                    quantity: 1,
                }],
            })
            .await
            .unwrap();

        let paid = svc.pay(placed.id).await.unwrap();
        assert_eq!(paid.state.status, OrderStatus::Paid);

        let shipped = svc.ship(placed.id).await.unwrap();
        assert_eq!(shipped.state.status, OrderStatus::Shipped);

        assert_eq!(
            *bus.published.lock().await,
            vec![OrderPlaced::SUBJECT, OrderPaid::SUBJECT, OrderShipped::SUBJECT]
        );
    }

    #[tokio::test]
    async fn cannot_ship_before_paying() {
        let (svc, read_model, _bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 100).await;

        let placed = svc
            .place(PlaceOrder {
                user_id,
                lines: vec![PlaceOrderLine {
                    product_id,
                    quantity: 1,
                }],
            })
            .await
            .unwrap();

        let err = svc.ship(placed.id).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn cannot_cancel_after_shipping() {
        let (svc, read_model, _bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 100).await;

        let placed = svc
            .place(PlaceOrder {
                user_id,
                lines: vec![PlaceOrderLine {
                    product_id,
                    quantity: 1,
                }],
            })
            .await
            .unwrap();
        svc.pay(placed.id).await.unwrap();
        svc.ship(placed.id).await.unwrap();

        let err = svc.cancel(placed.id).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn history_returns_the_raw_event_log() {
        let (svc, read_model, _bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 100).await;

        let placed = svc
            .place(PlaceOrder {
                user_id,
                lines: vec![PlaceOrderLine {
                    product_id,
                    quantity: 1,
                }],
            })
            .await
            .unwrap();
        svc.pay(placed.id).await.unwrap();

        let history = svc.history(placed.id).await.unwrap();
        assert_eq!(history.len(), 2);
    }
}
