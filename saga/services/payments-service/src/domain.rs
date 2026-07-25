//! Payments domain: a per-user wallet balance. Payments-service is the sole
//! owner of balances — orders-service never debits one directly, it only
//! asks (via a saga command) to charge an amount.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct UserId(pub Uuid);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
