//! The one new outbound seam versus a synchronous service: publishing domain
//! events to the broker. Kept behind a trait for the same reason the old
//! `UserDirectory` / `ProductCatalog` ports were traits in the microservices
//! lab — so unit tests can swap in a fake and never open a socket.

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
