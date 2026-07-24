//! Users domain — this service's private model. It is the sole owner of user
//! data; no other service may read this struct directly. Other services only
//! ever learn about a user through the `UserRegistered` event this service
//! chooses to publish (see events.rs).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub name: String,
}
