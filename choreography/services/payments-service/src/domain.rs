//! Payments domain: a per-user wallet balance. Payments-service is the sole
//! owner of balances — nobody else debits one, and in this lab nobody else
//! even asks. A wallet moves because this service noticed an order's stock
//! get reserved and decided that meant it was time.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct UserId(pub Uuid);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
