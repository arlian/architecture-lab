//! Orders use cases. `place` still needs the same two answers as the earlier
//! labs — does this user exist, what does each product cost — both still
//! answered from `read_model` (see read_model.rs), no network call.
//!
//! The change versus the saga lab is one line, and it's the entire point of
//! this architecture. `place` used to end by publishing
//! `ReserveStockRequested`: a command, addressed to inventory-service,
//! naming the step Orders had decided should happen next. It now ends by
//! publishing `orders.placed`: a fact, addressed to nobody, naming only what
//! Orders itself did.
//!
//! Orders-service no longer contains the sentence "and then stock gets
//! reserved" in any form. Grep it — there is no mention of stock, wallets,
//! inventory or payments anywhere except the subject strings in events.rs
//! that tracker.rs listens on for reporting. Whether placing an order
//! reserves stock at all is now entirely inventory-service's business.

use std::sync::Arc;

use crate::bus::EventBus;
use crate::domain::{Order, OrderId, OrderLine, OrderStatus, ProductId, UserId};
use crate::error::AppError;
use crate::events::{OrderPlaced, OrderPlacedLine};
use crate::read_model::ReadModel;
use crate::repository::OrderRepository;

pub struct PlaceOrderLine {
    pub product_id: ProductId,
    pub quantity: u32,
}

pub struct PlaceOrder {
    pub user_id: UserId,
    pub lines: Vec<PlaceOrderLine>,
}

pub struct OrderService {
    repo: Arc<dyn OrderRepository>,
    read_model: ReadModel,
    events: Arc<dyn EventBus>,
}

impl OrderService {
    pub fn new(
        repo: Arc<dyn OrderRepository>,
        read_model: ReadModel,
        events: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            repo,
            read_model,
            events,
        }
    }

    pub async fn place(&self, cmd: PlaceOrder) -> Result<Order, AppError> {
        if cmd.lines.is_empty() {
            return Err(AppError::Validation("an order needs at least one line".into()));
        }

        // A local lookup against our own projection of `users.registered` —
        // no network round-trip, but also no guarantee it's caught up yet.
        if !self.read_model.user_exists(cmd.user_id.0).await {
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
                .price_of(line.product_id.0)
                .await
                .ok_or_else(|| {
                    AppError::Validation(format!("product {} does not exist", line.product_id))
                })?;

            total_cents += unit_price_cents * line.quantity as u64;
            lines.push(OrderLine {
                product_id: line.product_id,
                quantity: line.quantity,
                unit_price_cents,
            });
        }

        let order = Order {
            id: OrderId::new(),
            user_id: cmd.user_id,
            lines,
            total_cents,
            status: OrderStatus::Pending,
            failure_reason: None,
        };
        let order = self.repo.insert(order).await;

        // Announce it and stop. We are not asking for anything and we are not
        // waiting for anyone. Whatever happens to this order next is a
        // decision made by services we have never heard of.
        let event = OrderPlaced {
            id: order.id.0,
            user_id: order.user_id.0,
            total_cents: order.total_cents,
            lines: order
                .lines
                .iter()
                .map(|l| OrderPlacedLine {
                    product_id: l.product_id.0,
                    quantity: l.quantity,
                })
                .collect(),
        };
        let payload = serde_json::to_vec(&event).expect("OrderPlaced is serializable");
        if let Err(e) = self.events.publish(OrderPlaced::SUBJECT, payload).await {
            // Worth noticing how bad this is. In the saga lab, a dropped
            // first command left an order stuck in `Pending` and the
            // orchestrator could, in principle, time it out and retry —
            // it knew a step was outstanding. Here, a dropped
            // `orders.placed` means no participant ever learns the order
            // exists, and no participant is expecting it, so nobody
            // anywhere is in a position to notice. See the README's
            // "known gaps".
            tracing::warn!("failed to publish OrderPlaced for order {}: {e}", order.id);
        }

        Ok(order)
    }

    pub async fn get(&self, id: OrderId) -> Result<Order, AppError> {
        self.repo
            .get(id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("order {id}")))
    }

    pub async fn list(&self) -> Vec<Order> {
        self.repo.all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::repository::InMemoryOrderRepository;

    // Because Orders depends on the read model (a plain struct, no socket) and
    // the `EventBus` trait, its tests stay exactly as network-free as the
    // microservices version's fakes were.
    #[derive(Default)]
    struct FakeBus {
        published: Mutex<Vec<(&'static str, Vec<u8>)>>,
    }

    #[async_trait::async_trait]
    impl EventBus for FakeBus {
        async fn publish(&self, subject: &'static str, payload: Vec<u8>) -> Result<(), AppError> {
            self.published.lock().await.push((subject, payload));
            Ok(())
        }
    }

    async fn service() -> (OrderService, ReadModel, Arc<FakeBus>) {
        let read_model = ReadModel::default();
        let bus = Arc::new(FakeBus::default());
        let svc = OrderService::new(
            Arc::new(InMemoryOrderRepository::default()),
            read_model.clone(),
            bus.clone(),
        );
        (svc, read_model, bus)
    }

    #[tokio::test]
    async fn totals_the_order() {
        let (svc, read_model, _bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 250).await;

        let order = svc
            .place(PlaceOrder {
                user_id: UserId(user_id),
                lines: vec![PlaceOrderLine {
                    product_id: ProductId(product_id),
                    quantity: 3,
                }],
            })
            .await
            .unwrap();

        assert_eq!(order.total_cents, 750);
        assert_eq!(order.status, OrderStatus::Pending);
    }

    #[tokio::test]
    async fn rejects_unknown_user() {
        let (svc, read_model, _bus) = service().await;
        let product_id = Uuid::new_v4();
        read_model.record_price(product_id, 250).await;

        let err = svc
            .place(PlaceOrder {
                user_id: UserId(Uuid::new_v4()),
                lines: vec![PlaceOrderLine {
                    product_id: ProductId(product_id),
                    quantity: 1,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_unknown_product() {
        let (svc, read_model, _bus) = service().await;
        let user_id = Uuid::new_v4();
        read_model.record_user(user_id).await;

        let err = svc
            .place(PlaceOrder {
                user_id: UserId(user_id),
                lines: vec![PlaceOrderLine {
                    product_id: ProductId(Uuid::new_v4()),
                    quantity: 1,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    /// The saga lab's equivalent test was named
    /// `placing_starts_the_saga_by_requesting_a_stock_reservation`. The
    /// rename is the architecture: Orders announces a fact and asserts
    /// nothing about what anyone does with it.
    #[tokio::test]
    async fn placing_announces_the_order_and_asks_for_nothing() {
        let (svc, read_model, bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 250).await;

        svc.place(PlaceOrder {
            user_id: UserId(user_id),
            lines: vec![PlaceOrderLine {
                product_id: ProductId(product_id),
                quantity: 2,
            }],
        })
        .await
        .unwrap();

        let published = bus.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, OrderPlaced::SUBJECT);
    }

    /// `orders.placed` has to carry enough for *every* downstream consumer,
    /// present and future, because none of them can call back to ask. This
    /// test pins that: the total is on the wire even though Orders has no
    /// idea anyone wants it.
    #[tokio::test]
    async fn the_placed_fact_carries_what_consumers_cannot_ask_for_later() {
        let (svc, read_model, bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 250).await;

        svc.place(PlaceOrder {
            user_id: UserId(user_id),
            lines: vec![PlaceOrderLine {
                product_id: ProductId(product_id),
                quantity: 2,
            }],
        })
        .await
        .unwrap();

        let published = bus.published.lock().await;
        let payload: serde_json::Value = serde_json::from_slice(&published[0].1).unwrap();
        assert_eq!(payload["user_id"], user_id.to_string());
        assert_eq!(payload["total_cents"], 500);
        assert_eq!(payload["lines"][0]["quantity"], 2);
    }
}
