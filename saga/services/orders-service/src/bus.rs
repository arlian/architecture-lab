//! Outbound seam: publishing `OrderPlaced`. Same trait-behind-a-fake pattern
//! as users-service/src/bus.rs and catalog-service/src/bus.rs, so
//! `OrderService`'s tests never open a socket.

use async_trait::async_trait;

use crate::error::AppError;

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, subject: &'static str, payload: Vec<u8>) -> Result<(), AppError>;
}

pub struct NatsBus {
    client: async_nats::Client,
}

impl NatsBus {
    pub fn new(client: async_nats::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl EventBus for NatsBus {
    async fn publish(&self, subject: &'static str, payload: Vec<u8>) -> Result<(), AppError> {
        self.client
            .publish(subject, payload.into())
            .await
            .map_err(|e| AppError::Internal(format!("failed to publish to {subject}: {e}")))
    }
}
