//! Orders use cases — the cross-context story, now told entirely with events.
//!
//! Placing an order still needs the same two answers as before:
//!   * does this user exist?
//!   * what does each product cost?
//! But both are now answered by looking at `read_model` — a local projection
//! built from events, not a network call. Note there's no `.await?` on those
//! lookups any more; a local read can't fail the way a socket can. What
//! replaces "the network is down" as a failure mode is staleness: the read
//! model might not have caught up yet. See read_model.rs.
//!
//! The one new outbound step is at the end: publish `OrderPlaced` so anyone
//! downstream (today: notifications-service) can react without Orders knowing
//! they exist.

use std::sync::Arc;

use crate::bus::EventBus;
use crate::domain::{Order, OrderId, OrderLine, ProductId, UserId};
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
    pub fn new(repo: Arc<dyn OrderRepository>, read_model: ReadModel, events: Arc<dyn EventBus>) -> Self {
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
        };
        let order = self.repo.insert(order).await;

        let event = OrderPlaced {
            id: order.id.0,
            user_id: order.user_id.0,
            lines: order
                .lines
                .iter()
                .map(|l| OrderPlacedLine {
                    product_id: l.product_id.0,
                    quantity: l.quantity,
                    unit_price_cents: l.unit_price_cents,
                })
                .collect(),
            total_cents: order.total_cents,
        };
        let payload = serde_json::to_vec(&event).expect("OrderPlaced is serializable");
        if let Err(e) = self.events.publish(OrderPlaced::SUBJECT, payload).await {
            tracing::warn!("failed to publish OrderPlaced for {}: {e}", order.id);
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

    #[tokio::test]
    async fn publishes_order_placed() {
        let (svc, read_model, bus) = service().await;
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();
        read_model.record_user(user_id).await;
        read_model.record_price(product_id, 250).await;

        svc.place(PlaceOrder {
            user_id: UserId(user_id),
            lines: vec![PlaceOrderLine {
                product_id: ProductId(product_id),
                quantity: 1,
            }],
        })
        .await
        .unwrap();

        let published = bus.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, OrderPlaced::SUBJECT);
    }
}
