//! Payments persistence — private, in-memory, owned by this service.
//!
//! Alongside wallet balances, this keeps a small idempotency ledger keyed by
//! `saga_id`: a redelivered `payments.charge.requested` for a saga already
//! applied must not charge the wallet twice. Same idempotency rule as
//! inventory-service's reservation ledger.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::UserId;

#[derive(Debug, Clone)]
pub(crate) enum ChargeOutcome {
    Charged(u64),
    Failed(String),
}

#[async_trait]
pub(crate) trait PaymentsRepository: Send + Sync {
    async fn open_wallet(&self, user_id: UserId, starting_balance_cents: u64);
    async fn balance(&self, user_id: UserId) -> Option<u64>;

    /// Charge `amount_cents` from `user_id`'s wallet for `saga_id`. Returns
    /// the outcome already recorded for `saga_id` unchanged if this is a
    /// redelivery, instead of charging a second time.
    async fn charge(&self, saga_id: Uuid, user_id: UserId, amount_cents: u64) -> ChargeOutcome;
}

#[derive(Default)]
pub(crate) struct InMemoryPaymentsRepository {
    wallets: RwLock<HashMap<UserId, u64>>,
    charges: RwLock<HashMap<Uuid, ChargeOutcome>>,
}

#[async_trait]
impl PaymentsRepository for InMemoryPaymentsRepository {
    async fn open_wallet(&self, user_id: UserId, starting_balance_cents: u64) {
        self.wallets.write().await.insert(user_id, starting_balance_cents);
    }

    async fn balance(&self, user_id: UserId) -> Option<u64> {
        self.wallets.read().await.get(&user_id).copied()
    }

    async fn charge(&self, saga_id: Uuid, user_id: UserId, amount_cents: u64) -> ChargeOutcome {
        if let Some(existing) = self.charges.read().await.get(&saga_id) {
            return existing.clone();
        }

        let mut wallets = self.wallets.write().await;
        let balance = wallets.get(&user_id).copied().unwrap_or(0);
        let outcome = if balance >= amount_cents {
            wallets.insert(user_id, balance - amount_cents);
            ChargeOutcome::Charged(amount_cents)
        } else {
            ChargeOutcome::Failed(format!(
                "wallet {user_id} has {balance}c, {amount_cents}c requested"
            ))
        };

        self.charges.write().await.insert(saga_id, outcome.clone());
        outcome
    }
}
