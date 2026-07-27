//! mobile-bff's aggregation — deliberately smaller than web-bff's.
//!
//! The mobile order-status screen only shows who the order is for, how many
//! items, and the total — so this BFF calls two backend services, not three,
//! and asks each of them for less. Compare with web-bff/src/views.rs, which
//! looks like it's building "the same" screen but talks to all three
//! services and fetches full product details per line. Same underlying
//! order, two genuinely different call graphs, because the clients differ.

use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::clients::{OrdersClient, UsersClient};
use crate::error::AppError;

#[derive(Debug, Serialize)]
pub struct OrderSummaryView {
    pub order_id: Uuid,
    pub customer_name: String,
    pub item_count: u32,
    pub total_cents: u64,
}

pub struct OrderAggregator {
    orders: Arc<dyn OrdersClient>,
    users: Arc<dyn UsersClient>,
}

impl OrderAggregator {
    pub fn new(orders: Arc<dyn OrdersClient>, users: Arc<dyn UsersClient>) -> Self {
        Self { orders, users }
    }

    pub async fn order_summary(&self, order_id: Uuid) -> Result<OrderSummaryView, AppError> {
        let order = self
            .orders
            .get(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("order {order_id}")))?;

        let user = self
            .users
            .get(order.user_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "order {order_id} references unknown user {}",
                    order.user_id
                ))
            })?;

        Ok(OrderSummaryView {
            order_id: order.id,
            customer_name: user.name,
            item_count: order.lines.iter().map(|l| l.quantity).sum(),
            total_cents: order.total_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use crate::clients::{OrderLineView, OrderView, PlaceOrderRequest, UserView};

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

    #[tokio::test]
    async fn sums_item_count_across_lines() {
        let order_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let orders = Arc::new(FakeOrders(Some(OrderView {
            id: order_id,
            user_id,
            lines: vec![OrderLineView { quantity: 2 }, OrderLineView { quantity: 1 }],
            total_cents: 1799,
        })));
        let users = Arc::new(FakeUsers(Some(UserView {
            id: user_id,
            name: "Ada".into(),
        })));

        let aggregator = OrderAggregator::new(orders, users);
        let view = aggregator.order_summary(order_id).await.unwrap();

        assert_eq!(view.customer_name, "Ada");
        assert_eq!(view.item_count, 3);
        assert_eq!(view.total_cents, 1799);
    }

    #[tokio::test]
    async fn missing_order_is_not_found() {
        let orders = Arc::new(FakeOrders(None));
        let users = Arc::new(FakeUsers(None));

        let aggregator = OrderAggregator::new(orders, users);
        let err = aggregator.order_summary(Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
