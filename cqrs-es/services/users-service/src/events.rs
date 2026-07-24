//! Events this service publishes. This is the entire public contract of an
//! event-driven service — there is no `GET /users/:id` for a neighbour to poll
//! any more. Consumers (today: orders-command-service) define their OWN copy of the
//! shape they care about; nothing here is imported anywhere else. Same rule as
//! the HTTP JSON contracts in the microservices lab, just over a different wire.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct UserRegistered {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

impl UserRegistered {
    pub const SUBJECT: &'static str = "users.registered";
}
