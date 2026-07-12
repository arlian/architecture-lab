//! Orders use cases — the cross-service story.
//!
//! Placing an order still collaborates with two neighbours:
//!   * confirm the customer exists   -> Users   (now an HTTP call)
//!   * look up each product's price   -> Catalog (now an HTTP call)
//!
//! The business logic is unchanged from the monolith. The one visible difference
//! is that the collaborator calls are fallible (`.await?`): a network hiccup on
//! the way to Users or Catalog now surfaces as an error instead of being
//! impossible. That is the essential trade of microservices — autonomy and
//! independent scaling, paid for with partial failure you must handle.

use std::sync::Arc;

use crate::clients::{ProductCatalog, UserDirectory};
use crate::domain::{Order, OrderId, OrderLine, ProductId, UserId};
use crate::error::AppError;
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
    users: Arc<dyn UserDirectory>,
    catalog: Arc<dyn ProductCatalog>,
}

impl OrderService {
    pub fn new(
        repo: Arc<dyn OrderRepository>,
        users: Arc<dyn UserDirectory>,
        catalog: Arc<dyn ProductCatalog>,
    ) -> Self {
        Self {
            repo,
            users,
            catalog,
        }
    }

    pub async fn place(&self, cmd: PlaceOrder) -> Result<Order, AppError> {
        if cmd.lines.is_empty() {
            return Err(AppError::Validation("an order needs at least one line".into()));
        }

        // Cross-service call #1: a real HTTP round-trip to the Users service.
        // The `?` propagates a network failure as AppError::Internal (502-ish).
        if !self.users.exists(cmd.user_id).await? {
            return Err(AppError::Validation(format!(
                "user {} does not exist",
                cmd.user_id
            )));
        }

        // Cross-service call #2: price every line via the Catalog service.
        let mut lines = Vec::with_capacity(cmd.lines.len());
        let mut total_cents: u64 = 0;
        for line in cmd.lines {
            if line.quantity == 0 {
                return Err(AppError::Validation("quantity must be at least 1".into()));
            }
            let unit_price_cents = self
                .catalog
                .price_of(line.product_id)
                .await?
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
        Ok(self.repo.insert(order).await)
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
    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::repository::InMemoryOrderRepository;

    // Because Orders depends on the *port traits*, its tests use tiny fakes and
    // never make a network call — exactly as in the monolith. The seam that made
    // the monolith testable is the same seam that lets us swap in HTTP clients.
    struct FakeUsers {
        exists: bool,
    }
    #[async_trait]
    impl UserDirectory for FakeUsers {
        async fn exists(&self, _id: UserId) -> Result<bool, AppError> {
            Ok(self.exists)
        }
    }

    struct FakeCatalog {
        price: Option<u64>,
    }
    #[async_trait]
    impl ProductCatalog for FakeCatalog {
        async fn price_of(&self, _id: ProductId) -> Result<Option<u64>, AppError> {
            Ok(self.price)
        }
    }

    fn service(users_ok: bool, price: Option<u64>) -> OrderService {
        OrderService::new(
            Arc::new(InMemoryOrderRepository::default()),
            Arc::new(FakeUsers { exists: users_ok }),
            Arc::new(FakeCatalog { price }),
        )
    }

    #[tokio::test]
    async fn totals_the_order() {
        let svc = service(true, Some(250));
        let order = svc
            .place(PlaceOrder {
                user_id: UserId(Uuid::new_v4()),
                lines: vec![PlaceOrderLine {
                    product_id: ProductId(Uuid::new_v4()),
                    quantity: 3,
                }],
            })
            .await
            .unwrap();

        assert_eq!(order.total_cents, 750);
    }

    #[tokio::test]
    async fn rejects_unknown_user() {
        let svc = service(false, Some(250));
        let err = svc
            .place(PlaceOrder {
                user_id: UserId(Uuid::new_v4()),
                lines: vec![PlaceOrderLine {
                    product_id: ProductId(Uuid::new_v4()),
                    quantity: 1,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
