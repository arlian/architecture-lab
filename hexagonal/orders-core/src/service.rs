//! Use cases — the driving port.
//!
//! Everything below is the same order logic as the other labs: an order needs
//! at least one line, the customer must exist, every line is priced from the
//! catalog at the moment of placing, and the total is the sum. Compare with
//! `microservices/services/orders-service/src/service.rs` and you'll find the
//! body of `place()` nearly identical.
//!
//! What changed is what this file is *allowed to know*. It has no `Json`, no
//! `StatusCode`, no `Path`, no `println!`. It cannot be reached over a
//! network, and it cannot print to a terminal. It takes a command, consults
//! its ports, and returns a value or a `DomainError`. Two entirely different
//! programs (`bin/serve.rs` and `bin/cli.rs`) drive exactly this code, and
//! neither is visible from here.
//!
//! ## Why is the driving port a struct, not a trait?
//!
//! Purist hexagonal would declare `trait PlaceOrderUseCase` and have the
//! adapters depend on that, so the core's *inbound* boundary is an interface
//! too. It buys real things (multiple use-case implementations, decorators
//! for logging or transactions) at the cost of another `Arc<dyn ...>` layer.
//! This lab is deliberately the small version: `OrderService`'s public
//! methods are the driving port, and the compiler still stops an adapter from
//! reaching past them, because everything behind them is private. Making it a
//! trait is the first exercise in the README.

use std::sync::Arc;

use crate::domain::{DomainError, Order, OrderId, OrderLine, ProductId, UserId};
use crate::ports::{OrderRepository, ProductCatalog, UserDirectory};

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
    /// The core states its needs as three ports and lets someone else satisfy
    /// them. Every caller of this constructor is a composition root.
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

    pub async fn place(&self, cmd: PlaceOrder) -> Result<Order, DomainError> {
        if cmd.lines.is_empty() {
            return Err(DomainError::Validation(
                "an order needs at least one line".into(),
            ));
        }

        if !self.users.exists(cmd.user_id).await? {
            return Err(DomainError::Validation(format!(
                "user {} does not exist",
                cmd.user_id
            )));
        }

        let mut lines = Vec::with_capacity(cmd.lines.len());
        let mut total_cents: u64 = 0;
        for line in cmd.lines {
            if line.quantity == 0 {
                return Err(DomainError::Validation("quantity must be at least 1".into()));
            }
            let unit_price_cents = self
                .catalog
                .price_of(line.product_id)
                .await?
                .ok_or_else(|| {
                    DomainError::Validation(format!("product {} does not exist", line.product_id))
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
        self.repo.insert(order).await
    }

    pub async fn get(&self, id: OrderId) -> Result<Order, DomainError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("order {id}")))
    }

    pub async fn list(&self) -> Result<Vec<Order>, DomainError> {
        self.repo.all().await
    }
}

#[cfg(test)]
mod tests {
    //! These tests are the third driving adapter in the lab, and the cheapest
    //! proof that the hexagon is sealed: the whole order workflow is
    //! exercised here with no server, no file, and no port number. The fakes
    //! below are not a testing trick — they are adapters, exactly as real as
    //! the ones in `orders-app`, and the core cannot tell the difference.

    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct FakeRepo {
        orders: Mutex<Vec<Order>>,
    }

    #[async_trait]
    impl OrderRepository for FakeRepo {
        async fn insert(&self, order: Order) -> Result<Order, DomainError> {
            self.orders.lock().unwrap().push(order.clone());
            Ok(order)
        }
        async fn get(&self, id: OrderId) -> Result<Option<Order>, DomainError> {
            Ok(self
                .orders
                .lock()
                .unwrap()
                .iter()
                .find(|o| o.id == id)
                .cloned())
        }
        async fn all(&self) -> Result<Vec<Order>, DomainError> {
            Ok(self.orders.lock().unwrap().clone())
        }
    }

    /// A repository that is always broken — one small struct, and now the
    /// core's behaviour under a dead database is a unit test instead of a
    /// staging incident.
    struct BrokenRepo;

    #[async_trait]
    impl OrderRepository for BrokenRepo {
        async fn insert(&self, _order: Order) -> Result<Order, DomainError> {
            Err(DomainError::Unavailable("storage is down".into()))
        }
        async fn get(&self, _id: OrderId) -> Result<Option<Order>, DomainError> {
            Err(DomainError::Unavailable("storage is down".into()))
        }
        async fn all(&self) -> Result<Vec<Order>, DomainError> {
            Err(DomainError::Unavailable("storage is down".into()))
        }
    }

    struct FakeUsers {
        exists: bool,
    }
    #[async_trait]
    impl UserDirectory for FakeUsers {
        async fn exists(&self, _id: UserId) -> Result<bool, DomainError> {
            Ok(self.exists)
        }
    }

    struct FakeCatalog {
        price: Option<u64>,
    }
    #[async_trait]
    impl ProductCatalog for FakeCatalog {
        async fn price_of(&self, _id: ProductId) -> Result<Option<u64>, DomainError> {
            Ok(self.price)
        }
    }

    fn service(repo: Arc<dyn OrderRepository>, users_ok: bool, price: Option<u64>) -> OrderService {
        OrderService::new(
            repo,
            Arc::new(FakeUsers { exists: users_ok }),
            Arc::new(FakeCatalog { price }),
        )
    }

    fn one_line(quantity: u32) -> PlaceOrder {
        PlaceOrder {
            user_id: UserId(Uuid::new_v4()),
            lines: vec![PlaceOrderLine {
                product_id: ProductId(Uuid::new_v4()),
                quantity,
            }],
        }
    }

    #[tokio::test]
    async fn totals_the_order() {
        let svc = service(Arc::new(FakeRepo::default()), true, Some(250));
        let order = svc.place(one_line(3)).await.unwrap();
        assert_eq!(order.total_cents, 750);
    }

    #[tokio::test]
    async fn rejects_unknown_user() {
        let svc = service(Arc::new(FakeRepo::default()), false, Some(250));
        let err = svc.place(one_line(1)).await.unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_unknown_product() {
        let svc = service(Arc::new(FakeRepo::default()), true, None);
        let err = svc.place(one_line(1)).await.unwrap_err();
        assert!(matches!(err, DomainError::Validation(_)));
    }

    #[tokio::test]
    async fn reports_storage_failure_as_unavailable() {
        let svc = service(Arc::new(BrokenRepo), true, Some(250));
        let err = svc.place(one_line(1)).await.unwrap_err();
        assert!(matches!(err, DomainError::Unavailable(_)));
    }

    #[tokio::test]
    async fn missing_order_is_not_found() {
        let svc = service(Arc::new(FakeRepo::default()), true, Some(250));
        let err = svc.get(OrderId::new()).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
