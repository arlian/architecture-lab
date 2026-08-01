//! Outbound seam: publishing `orders.placed`. Same trait-behind-a-fake
//! pattern as every other service in this lab, so `OrderService`'s tests
//! never open a socket.
//!
//! This is the only outbound edge orders-service has left. In the saga lab
//! the same seam carried four different saga commands out to two
//! participants.

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
