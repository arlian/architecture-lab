//! The same contract as naive-service, with two fields that are no longer
//! decorative:
//!
//! ```text
//! GET  /stock/:product                  -> 200 { strategy, product, available, version }
//! POST /stock/:product/seed    {units}  -> 200 { strategy, product, available, version }
//! POST /stock/:product/reserve {units, expected_version?}
//!                                       -> 200 { strategy, product, reserved, available, version, attempts }
//!                                       -> 404 unknown product
//!                                       -> 409 the row moved (see below)
//!                                       -> 422 not enough stock
//! ```
//!
//! * `version` in the response is what a client feeds back as
//!   `expected_version` on its next write. Together they are a read-modify-write
//!   protocol the client can participate in — the HTTP equivalent of `ETag` and
//!   `If-Match`, which is worth knowing is the *same idea*, standardised.
//! * `attempts` says how many times the server quietly started over. It is the
//!   cost of optimism, made visible rather than hidden in a latency histogram.
//!
//! The 409 is the interesting status code, because it is the one a client has
//! to have an opinion about. Retrying it blindly is usually right and
//! occasionally catastrophic: a retried *payment* is not the same as a retried
//! *read*. That question — "is this operation safe to repeat?" — is idempotency,
//! and it is a different problem from this lab's; see `saga/`, where every
//! command carries a `saga_id` for exactly that reason.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::store::Store;

pub const STRATEGY: &str = "optimistic";

#[derive(Deserialize)]
struct SeedBody {
    units: u64,
}

#[derive(Deserialize)]
struct ReserveBody {
    units: u64,
    /// When present, the server will not retry on the client's behalf: the
    /// client asked to be told about conflicts, so it gets told.
    #[serde(default)]
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
    attempts: u32,
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

    let reservation = store
        .reserve(&product, body.units, body.expected_version)
        .await?;
    tracing::debug!(
        product = %product,
        reserved = reservation.reserved,
        available = reservation.available,
        attempts = reservation.attempts,
        "reserved"
    );

    Ok(Json(ReserveView {
        strategy: STRATEGY,
        product,
        reserved: reservation.reserved,
        available: reservation.available,
        version: reservation.version,
        attempts: reservation.attempts,
    }))
}
