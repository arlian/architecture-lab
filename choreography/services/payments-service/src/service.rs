//! Payments use cases.
//!
//! The saga lab's version of this file had one entry point — `charge`, called
//! with everything it needed, straight off the orchestrator's command. This
//! one has two, `on_order_placed` and `on_stock_reserved`, and neither of
//! them is "charge". Both are "here is half of what I need; charge if that
//! completes the pair."
//!
//! Nothing about the payments *domain* got harder. Charging a wallet is the
//! same three lines it always was. What got harder is finding out that a
//! charge should happen at all, and assembling the arguments for it. That
//! work used to be the orchestrator's, and deleting the orchestrator did not
//! delete the work — it moved it in here, and (in a different shape) into
//! notifications-service, and into orders-service's tracker.

use std::sync::Arc;
use uuid::Uuid;

use serde::Serialize;

use crate::bus::EventBus;
use crate::domain::UserId;
use crate::events::{PaymentCharged, PaymentDeclined};
use crate::repository::{ChargeOutcome, PaymentsRepository, PlacedOrder};

pub struct PaymentsService {
    repo: Arc<dyn PaymentsRepository>,
    events: Arc<dyn EventBus>,
}

impl PaymentsService {
    pub fn new(repo: Arc<dyn PaymentsRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repo, events }
    }

    pub async fn open_wallet(&self, user_id: UserId, starting_balance_cents: u64) {
        self.repo.open_wallet(user_id, starting_balance_cents).await;
    }

    pub async fn balance(&self, user_id: UserId) -> Option<u64> {
        self.repo.balance(user_id).await
    }

    /// Heard `orders.placed`. On its own this means nothing to payments — an
    /// order that never gets its stock reserved must never be charged. File
    /// the details away; charge only if a reservation was already waiting on
    /// them.
    pub async fn on_order_placed(&self, order_id: Uuid, user_id: UserId, amount_cents: u64) {
        let order = PlacedOrder {
            user_id,
            amount_cents,
        };
        if let Some(order) = self.repo.join_placed(order_id, order).await {
            tracing::debug!(
                "order {order_id} details arrived after its reservation; charging now"
            );
            self.charge(order_id, order).await;
        }
    }

    /// Heard `inventory.stock_reserved`. This is the trigger — but it carries
    /// no user and no amount, so it can only be acted on if `orders.placed`
    /// has already been seen.
    pub async fn on_stock_reserved(&self, order_id: Uuid) {
        match self.repo.join_reserved(order_id).await {
            Some(order) => self.charge(order_id, order).await,
            None => tracing::debug!(
                "stock reserved for order {order_id} but its details haven't arrived yet; parking"
            ),
        }
    }

    async fn charge(&self, order_id: Uuid, order: PlacedOrder) {
        match self
            .repo
            .charge(order_id, order.user_id, order.amount_cents)
            .await
        {
            ChargeOutcome::Charged(_) => {
                self.publish(PaymentCharged::SUBJECT, &PaymentCharged { order_id })
                    .await;
            }
            ChargeOutcome::Declined(reason) => {
                self.publish(
                    PaymentDeclined::SUBJECT,
                    &PaymentDeclined { order_id, reason },
                )
                .await;
            }
        }
    }

    async fn publish<T: Serialize>(&self, subject: &'static str, event: &T) {
        let payload = serde_json::to_vec(event).expect("event is serializable");
        if let Err(e) = self.events.publish(subject, payload).await {
            tracing::warn!("failed to publish {subject}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    use crate::error::AppError;
    use crate::repository::InMemoryPaymentsRepository;

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

    fn service() -> (PaymentsService, Arc<FakeBus>) {
        let bus = Arc::new(FakeBus::default());
        let svc = PaymentsService::new(
            Arc::new(InMemoryPaymentsRepository::default()),
            bus.clone(),
        );
        (svc, bus)
    }

    #[tokio::test]
    async fn charges_when_balance_sufficient() {
        let (svc, bus) = service();
        let user_id = UserId(Uuid::new_v4());
        let order_id = Uuid::new_v4();
        svc.open_wallet(user_id, 2000).await;

        svc.on_order_placed(order_id, user_id, 1299).await;
        svc.on_stock_reserved(order_id).await;

        assert_eq!(svc.balance(user_id).await, Some(701));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, PaymentCharged::SUBJECT);
    }

    #[tokio::test]
    async fn declines_when_balance_insufficient() {
        let (svc, bus) = service();
        let user_id = UserId(Uuid::new_v4());
        let order_id = Uuid::new_v4();
        svc.open_wallet(user_id, 2000).await;

        svc.on_order_placed(order_id, user_id, 2598).await;
        svc.on_stock_reserved(order_id).await;

        assert_eq!(svc.balance(user_id).await, Some(2000));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, PaymentDeclined::SUBJECT);
    }

    /// A placed order that never gets its stock reserved must never be
    /// charged. Placement alone is not the trigger — this is what stops
    /// payments from charging for an order inventory rejected.
    #[tokio::test]
    async fn does_not_charge_on_placement_alone() {
        let (svc, bus) = service();
        let user_id = UserId(Uuid::new_v4());
        svc.open_wallet(user_id, 2000).await;

        svc.on_order_placed(Uuid::new_v4(), user_id, 1299).await;

        assert_eq!(svc.balance(user_id).await, Some(2000));
        assert!(bus.published.lock().await.is_empty());
    }

    /// The race the join exists for: `inventory.stock_reserved` observed
    /// *before* `orders.placed`, because the two subjects are drained by
    /// independent tasks. A plain lookup-on-trigger would drop this charge on
    /// the floor and strand the order.
    #[tokio::test]
    async fn charges_when_the_reservation_is_observed_before_the_order() {
        let (svc, bus) = service();
        let user_id = UserId(Uuid::new_v4());
        let order_id = Uuid::new_v4();
        svc.open_wallet(user_id, 2000).await;

        svc.on_stock_reserved(order_id).await; // arrives first
        assert!(
            bus.published.lock().await.is_empty(),
            "nothing to charge yet — parked"
        );

        svc.on_order_placed(order_id, user_id, 1299).await;

        assert_eq!(svc.balance(user_id).await, Some(701));
        let published = bus.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, PaymentCharged::SUBJECT);
    }

    #[tokio::test]
    async fn charge_is_idempotent_by_order_id() {
        let (svc, _bus) = service();
        let user_id = UserId(Uuid::new_v4());
        let order_id = Uuid::new_v4();
        svc.open_wallet(user_id, 2000).await;

        svc.on_order_placed(order_id, user_id, 1299).await;
        svc.on_stock_reserved(order_id).await;
        svc.on_stock_reserved(order_id).await; // redelivery

        assert_eq!(svc.balance(user_id).await, Some(701));
    }

    /// Both halves redelivered, in the awkward order. The join must not
    /// re-arm and fire a second charge.
    #[tokio::test]
    async fn a_redelivered_placement_does_not_re_arm_the_join() {
        let (svc, _bus) = service();
        let user_id = UserId(Uuid::new_v4());
        let order_id = Uuid::new_v4();
        svc.open_wallet(user_id, 2000).await;

        svc.on_stock_reserved(order_id).await;
        svc.on_order_placed(order_id, user_id, 1299).await; // completes the join
        svc.on_order_placed(order_id, user_id, 1299).await; // redelivery

        assert_eq!(svc.balance(user_id).await, Some(701));
    }
}
