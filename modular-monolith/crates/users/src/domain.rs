//! The domain layer: the entities and value objects this module owns.
//! No framework, no persistence, no HTTP — just the concepts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A strongly-typed identifier. Using a newtype instead of a raw `Uuid` means
/// you can never accidentally pass an `OrderId` where a `UserId` is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The User entity.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub name: String,
}
