//! Outbound HTTP clients for the three backend services web-bff depends on.
//!
//! Each client exposes exactly the narrow slice of the backend's contract this
//! BFF actually needs — the same "port" discipline the other labs use, just
//! aimed at whole services instead of in-process traits. There is no shared
//! `clients` crate between web-bff and mobile-bff: mobile-bff doesn't even
//! define a catalog client, because its screen never needs one. See
//! mobile-bff/src/clients.rs for the contrast.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct OrderLineView {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
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
    /// A pure proxy — not every BFF endpoint needs aggregation. Placing an
    /// order is exactly what orders-service already does; web-bff just
    /// forwards the request and hands back the response unchanged.
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
    pub email: String,
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

// ---------------------------------------------------------------------------
// Catalog — web-bff only. mobile-bff has no equivalent of this client.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ProductView {
    pub id: Uuid,
    pub name: String,
    pub price_cents: u64,
}

#[async_trait]
pub trait CatalogClient: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<ProductView>, AppError>;
}

pub struct CatalogHttpClient {
    base_url: String,
    http: reqwest::Client,
}

impl CatalogHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl CatalogClient for CatalogHttpClient {
    async fn get(&self, id: Uuid) -> Result<Option<ProductView>, AppError> {
        let url = format!("{}/products/{}", self.base_url, id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("catalog service unreachable: {e}")))?;

        let status = resp.status();
        if status.is_success() {
            let view = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(format!("catalog service sent bad JSON: {e}")))?;
            Ok(Some(view))
        } else if status.as_u16() == 404 {
            Ok(None)
        } else {
            Err(AppError::Internal(format!(
                "catalog service returned {status}"
            )))
        }
    }
}
