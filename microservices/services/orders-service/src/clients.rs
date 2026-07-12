//! Outbound adapters: how Orders reaches its neighbours.
//!
//! In the modular monolith, Orders depended on the traits `UserDirectory` and
//! `ProductCatalog`, and the composition root injected the *real service objects*
//! — an in-process function call that could never fail.
//!
//! Here the ports look almost identical, but:
//!   * the implementations are **HTTP clients** pointed at another service's URL;
//!   * every method returns `Result`, because the network can be slow, down, or
//!     return something unexpected. Handling that is the tax microservices charge.
//!
//! Keeping the ports as traits still pays off: `OrderService` depends on the
//! traits, so its unit tests substitute in-memory fakes and never open a socket.

use async_trait::async_trait;

use crate::domain::{ProductId, UserId};
use crate::error::AppError;

/// "Does this user exist?" — the only thing Orders needs from Users.
#[async_trait]
pub trait UserDirectory: Send + Sync {
    async fn exists(&self, id: UserId) -> Result<bool, AppError>;
}

/// "What does this product cost?" — the only thing Orders needs from Catalog.
#[async_trait]
pub trait ProductCatalog: Send + Sync {
    async fn price_of(&self, id: ProductId) -> Result<Option<u64>, AppError>;
}

/// HTTP client for the Users service.
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
impl UserDirectory for UsersHttpClient {
    async fn exists(&self, id: UserId) -> Result<bool, AppError> {
        // Existence is encoded in the status code of GET /users/:id.
        let url = format!("{}/users/{}", self.base_url, id.0);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::Internal(format!("users service unreachable: {e}"))
        })?;

        let status = resp.status();
        if status.is_success() {
            Ok(true)
        } else if status.as_u16() == 404 {
            Ok(false)
        } else {
            Err(AppError::Internal(format!(
                "users service returned {status}"
            )))
        }
    }
}

/// HTTP client for the Catalog service.
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

/// We deserialize only the field we need out of Catalog's product JSON. Orders
/// has no business knowing the rest of the product shape — the narrow contract
/// survives the jump to HTTP.
#[derive(serde::Deserialize)]
struct PriceView {
    price_cents: u64,
}

#[async_trait]
impl ProductCatalog for CatalogHttpClient {
    async fn price_of(&self, id: ProductId) -> Result<Option<u64>, AppError> {
        let url = format!("{}/products/{}", self.base_url, id.0);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            AppError::Internal(format!("catalog service unreachable: {e}"))
        })?;

        let status = resp.status();
        if status.is_success() {
            let view: PriceView = resp.json().await.map_err(|e| {
                AppError::Internal(format!("catalog service sent bad JSON: {e}"))
            })?;
            Ok(Some(view.price_cents))
        } else if status.as_u16() == 404 {
            Ok(None)
        } else {
            Err(AppError::Internal(format!(
                "catalog service returned {status}"
            )))
        }
    }
}
