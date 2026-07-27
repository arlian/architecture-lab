//! The aggregation itself — the entire reason web-bff exists.
//!
//! An order-detail screen needs data owned by three different services: the
//! order (orders-service), the customer's name and email (users-service), and
//! each line's product name (catalog-service). Without a BFF, either the
//! browser makes all of those calls itself (chatty, and every client repeats
//! the same orchestration), or orders-service grows a `?include=user,products`
//! parameter that only web needs and mobile pays for anyway. Here that
//! fan-out and reshaping happens in exactly one place.

use std::sync::Arc;

use futures::future::try_join_all;
use serde::Serialize;
use uuid::Uuid;

use crate::clients::{CatalogClient, OrdersClient, UsersClient};
use crate::error::AppError;

#[derive(Debug, Serialize)]
pub struct CustomerView {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct OrderLineDetail {
    pub product_id: Uuid,
    pub product_name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    pub line_total_cents: u64,
}

#[derive(Debug, Serialize)]
pub struct OrderDetailView {
    pub order_id: Uuid,
    pub customer: CustomerView,
    pub lines: Vec<OrderLineDetail>,
    pub total_cents: u64,
}

pub struct OrderAggregator {
    orders: Arc<dyn OrdersClient>,
    users: Arc<dyn UsersClient>,
    catalog: Arc<dyn CatalogClient>,
}

impl OrderAggregator {
    pub fn new(
        orders: Arc<dyn OrdersClient>,
        users: Arc<dyn UsersClient>,
        catalog: Arc<dyn CatalogClient>,
    ) -> Self {
        Self {
            orders,
            users,
            catalog,
        }
    }

    /// web-bff is the only thing in this lab that calls all three backend
    /// services to answer a single request — that fan-out is its whole job.
    pub async fn order_detail(&self, order_id: Uuid) -> Result<OrderDetailView, AppError> {
        let order = self
            .orders
            .get(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("order {order_id}")))?;

        // The customer lookup and every line's product lookup are independent
        // of each other, so they run concurrently instead of one round trip
        // at a time. That parallel fan-out — hidden behind one HTTP request
        // from the client — is the other half of what a BFF buys you, beyond
        // just reshaping the response.
        let user_id = order.user_id;
        let users = self.users.clone();
        let user_fut = async move {
            users
                .get(user_id)
                .await?
                .ok_or_else(|| AppError::Internal(format!("order {order_id} references unknown user {user_id}")))
        };

        let catalog = self.catalog.clone();
        let lines = order.lines.clone();
        let lines_fut = async move {
            try_join_all(lines.into_iter().map(|line| {
                let catalog = catalog.clone();
                async move {
                    let product = catalog.get(line.product_id).await?.ok_or_else(|| {
                        AppError::Internal(format!(
                            "order {order_id} references unknown product {}",
                            line.product_id
                        ))
                    })?;
                    Ok::<_, AppError>(OrderLineDetail {
                        product_id: line.product_id,
                        product_name: product.name,
                        quantity: line.quantity,
                        unit_price_cents: line.unit_price_cents,
                        line_total_cents: line.unit_price_cents * line.quantity as u64,
                    })
                }
            }))
            .await
        };

        let (user, lines) = futures::try_join!(user_fut, lines_fut)?;

        Ok(OrderDetailView {
            order_id: order.id,
            customer: CustomerView {
                id: user.id,
                name: user.name,
                email: user.email,
            },
            lines,
            total_cents: order.total_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    use crate::clients::{OrderLineView, OrderView, PlaceOrderRequest, ProductView, UserView};

    struct FakeOrders(Option<OrderView>);
    #[async_trait]
    impl OrdersClient for FakeOrders {
        async fn place(&self, _req: PlaceOrderRequest) -> Result<OrderView, AppError> {
            unimplemented!("not exercised by these tests")
        }
        async fn get(&self, _id: Uuid) -> Result<Option<OrderView>, AppError> {
            Ok(self.0.clone())
        }
    }

    struct FakeUsers(Option<UserView>);
    #[async_trait]
    impl UsersClient for FakeUsers {
        async fn get(&self, _id: Uuid) -> Result<Option<UserView>, AppError> {
            Ok(self.0.clone())
        }
    }

    struct FakeCatalog(HashMap<Uuid, ProductView>);
    #[async_trait]
    impl CatalogClient for FakeCatalog {
        async fn get(&self, id: Uuid) -> Result<Option<ProductView>, AppError> {
            Ok(self.0.get(&id).cloned())
        }
    }

    #[tokio::test]
    async fn aggregates_customer_and_named_lines() {
        let order_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let product_id = Uuid::new_v4();

        let orders = Arc::new(FakeOrders(Some(OrderView {
            id: order_id,
            user_id,
            lines: vec![OrderLineView {
                product_id,
                quantity: 2,
                unit_price_cents: 500,
            }],
            total_cents: 1000,
        })));
        let users = Arc::new(FakeUsers(Some(UserView {
            id: user_id,
            email: "ada@example.com".into(),
            name: "Ada".into(),
        })));
        let mut products = HashMap::new();
        products.insert(
            product_id,
            ProductView {
                id: product_id,
                name: "Coffee Mug".into(),
                price_cents: 500,
            },
        );
        let catalog = Arc::new(FakeCatalog(products));

        let aggregator = OrderAggregator::new(orders, users, catalog);
        let view = aggregator.order_detail(order_id).await.unwrap();

        assert_eq!(view.customer.name, "Ada");
        assert_eq!(view.lines.len(), 1);
        assert_eq!(view.lines[0].product_name, "Coffee Mug");
        assert_eq!(view.lines[0].line_total_cents, 1000);
        assert_eq!(view.total_cents, 1000);
    }

    #[tokio::test]
    async fn missing_order_is_not_found() {
        let orders = Arc::new(FakeOrders(None));
        let users = Arc::new(FakeUsers(None));
        let catalog = Arc::new(FakeCatalog(HashMap::new()));

        let aggregator = OrderAggregator::new(orders, users, catalog);
        let err = aggregator.order_detail(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
