//! Outbound HTTP clients for the backend services mobile-bff depends on.
//!
//! Notice there is no `CatalogClient` here, unlike web-bff/src/clients.rs.
//! mobile-bff's one screen shows an item count and a total, never a product
//! name, so this service has no reason to know catalog-service exists — not
//! "know about it but skip calling it," but genuinely no client for it at
//! all. That's the point: each BFF's dependencies are shaped by its client's
//! needs, not by "every backend service, just in case."

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct OrderLineView {
    pub quantity: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrderView {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lines: Vec<OrderLineView>,
    pub total_cents: u64,
}

#[derive(Serialize)]
pub struct PlaceOrderLineRequest {
    pub product_id: Uuid,
    pub quantity: u32,
}

#[derive(Serialize)]
pub struct PlaceOrderRequest {
    pub user_id: Uuid,
    pub lines: Vec<PlaceOrderLineRequest>,
}

#[async_trait]
pub trait OrdersClient: Send + Sync {
    /// A pure proxy, same as web-bff's: forwards the request, hands back the
    /// response unchanged. No aggregation needed to place an order.
    async fn place(&self, req: PlaceOrderRequest) -> Result<OrderView, AppError>;
    async fn get(&self, id: Uuid) -> Result<Option<OrderView>, AppError>;
}

pub struct OrdersHttpClient {
    base_url: String,
    http: reqwest::Client,
}

impl OrdersHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OrdersClient for OrdersHttpClient {
    async fn place(&self, req: PlaceOrderRequest) -> Result<OrderView, AppError> {
        let url = format!("{}/orders", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("orders service unreachable: {e}")))?;

        if resp.status().is_success() {
            resp.json()
                .await
                .map_err(|e| AppError::Internal(format!("orders service sent bad JSON: {e}")))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::Validation(format!(
                "orders service rejected the order ({status}): {body}"
            )))
        }
    }

    async fn get(&self, id: Uuid) -> Result<Option<OrderView>, AppError> {
        let url = format!("{}/orders/{}", self.base_url, id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("orders service unreachable: {e}")))?;

        let status = resp.status();
        if status.is_success() {
            let view = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("orders service sent bad JSON: {e}")))?;
            Ok(Some(view))
        } else if status.as_u16() == 404 {
            Ok(None)
        } else {
            Err(AppError::Internal(format!(
                "orders service returned {status}"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct UserView {
    pub id: Uuid,
    pub name: String,
}

#[async_trait]
pub trait UsersClient: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<UserView>, AppError>;
}

pub struct UsersHttpClient {
    base_url: String,
    http: reqwest::Client,
}

impl UsersHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl UsersClient for UsersHttpClient {
    async fn get(&self, id: Uuid) -> Result<Option<UserView>, AppError> {
        let url = format!("{}/users/{}", self.base_url, id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("users service unreachable: {e}")))?;

        let status = resp.status();
        if status.is_success() {
            let view = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("users service sent bad JSON: {e}")))?;
            Ok(Some(view))
        } else if status.as_u16() == 404 {
            Ok(None)
        } else {
            Err(AppError::Internal(format!(
                "users service returned {status}"
            )))
        }
    }
}
