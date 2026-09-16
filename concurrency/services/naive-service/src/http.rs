//! The contract. All four services in this lab expose exactly this surface,
//! byte for byte, so the race-runner can be pointed at any of them without
//! knowing which one it hit:
//!
//! ```text
//! GET  /stock/:product                  -> 200 { strategy, product, available, version }
//! POST /stock/:product/seed    {units}  -> 200 { strategy, product, available, version }
//! POST /stock/:product/reserve {units, expected_version?}
//!                                       -> 200 { strategy, product, reserved, available, version }
//!                                       -> 404 unknown product
//!                                       -> 422 not enough stock
//! ```
//!
//! Two things are worth noticing about *this* service's version of it.
//!
//! First, `expected_version` is accepted and ignored. A client that tries to do
//! the right thing — "only apply this if the row is still at version 7" — gets
//! no error, no warning, and no protection. Silently ignoring a precondition is
//! worse than not offering one, and it is a real failure mode of hand-rolled
//! APIs: the field exists because someone added it to the struct, and nothing
//! downstream ever looked at it.
//!
//! Second, there is no 409 in the list. That is not an omission in the docs.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::store::Store;

pub const STRATEGY: &str = "naive";

#[derive(Deserialize)]
struct SeedBody {
    units: u64,
}

#[derive(Deserialize)]
struct ReserveBody {
    units: u64,
    /// Accepted for contract compatibility with optimistic-service. Never read.
    #[serde(default)]
    #[allow(dead_code)]
    expected_version: Option<u64>,
}

#[derive(Serialize)]
struct StockView {
    strategy: &'static str,
    product: String,
    available: u64,
    version: u64,
}

#[derive(Serialize)]
struct ReserveView {
    strategy: &'static str,
    product: String,
    reserved: u64,
    available: u64,
    version: u64,
}

pub fn router(store: Arc<Store>) -> Router {
    Router::new()
        .route("/stock/:product", get(read_stock))
        .route("/stock/:product/seed", post(seed))
        .route("/stock/:product/reserve", post(reserve))
        .with_state(store)
}

async fn read_stock(
    State(store): State<Arc<Store>>,
    Path(product): Path<String>,
) -> Result<Json<StockView>, AppError> {
    let entry = store
        .get(&product)
        .await
        .ok_or_else(|| crate::error::ReserveError::UnknownProduct(product.clone()))?;

    Ok(Json(StockView {
        strategy: STRATEGY,
        product,
        available: entry.units,
        version: entry.version,
    }))
}

async fn seed(
    State(store): State<Arc<Store>>,
    Path(product): Path<String>,
    Json(body): Json<SeedBody>,
) -> Result<Json<StockView>, AppError> {
    let entry = store.seed(&product, body.units).await;
    tracing::info!(product = %product, units = body.units, "seeded");

    Ok(Json(StockView {
        strategy: STRATEGY,
        product,
        available: entry.units,
        version: entry.version,
    }))
}

async fn reserve(
    State(store): State<Arc<Store>>,
    Path(product): Path<String>,
    Json(body): Json<ReserveBody>,
) -> Result<Json<ReserveView>, AppError> {
    if body.units == 0 {
        return Err(AppError::Validation("`units` must be greater than zero".into()));
    }

    let reservation = store.reserve(&product, body.units).await?;
    tracing::debug!(
        product = %product,
        reserved = reservation.reserved,
        available = reservation.available,
        "reserved"
    );

    Ok(Json(ReserveView {
        strategy: STRATEGY,
        product,
        reserved: reservation.reserved,
        available: reservation.available,
        version: reservation.version,
    }))
}
