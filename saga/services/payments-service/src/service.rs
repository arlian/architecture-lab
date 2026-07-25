//! Payments use cases — this service's half of the saga conversation.
//! `charge` is idempotent by `saga_id`, so a redelivered command from the
//! orchestrator (because it never saw a lost reply) can't double-charge a
//! wallet. It publishes its own reply event; orders-service's saga reactor
//! is the only thing waiting on it.

use std::sync::Arc;
use uuid::Uuid;

use serde::Serialize;

use crate::bus::EventBus;
use crate::domain::UserId;
use crate::events::{PaymentCharged, PaymentChargeFailed};
use crate::repository::{ChargeOutcome, PaymentsRepository};

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

    pub async fn charge(&self, saga_id: Uuid, user_id: UserId, amount_cents: u64) {
        match self.repo.charge(saga_id, user_id, amount_cents).await {
            ChargeOutcome::Charged(_) => {
                self.publish(PaymentCharged::SUBJECT, &PaymentCharged { saga_id })
                    .await;
            }
            ChargeOutcome::Failed(reason) => {
                self.publish(
                    PaymentChargeFailed::SUBJECT,
                    &PaymentChargeFailed { saga_id, reason },
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
        svc.open_wallet(user_id, 2000).await;

        svc.charge(Uuid::new_v4(), user_id, 1299).await;

        assert_eq!(svc.balance(user_id).await, Some(701));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, PaymentCharged::SUBJECT);
    }

    #[tokio::test]
    async fn fails_when_balance_insufficient() {
        let (svc, bus) = service();
        let user_id = UserId(Uuid::new_v4());
        svc.open_wallet(user_id, 2000).await;

        svc.charge(Uuid::new_v4(), user_id, 2598).await;

        assert_eq!(svc.balance(user_id).await, Some(2000));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, PaymentChargeFailed::SUBJECT);
    }

    #[tokio::test]
    async fn charge_is_idempotent_by_saga_id() {
        let (svc, _bus) = service();
        let user_id = UserId(Uuid::new_v4());
        svc.open_wallet(user_id, 2000).await;
        let saga_id = Uuid::new_v4();

        svc.charge(saga_id, user_id, 1299).await;
        svc.charge(saga_id, user_id, 1299).await; // redelivery

        assert_eq!(svc.balance(user_id).await, Some(701));
    }
}
