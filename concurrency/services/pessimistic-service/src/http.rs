//! The same contract again:
//!
//! ```text
//! GET  /stock/:product                  -> 200 { strategy, product, available, version }
//! POST /stock/:product/seed    {units}  -> 200 { strategy, product, available, version }
//! POST /stock/:product/reserve {units, expected_version?}
//!                                       -> 200 { strategy, product, reserved, available, version, waited_ms }
//!                                       -> 404 unknown product
//!                                       -> 422 not enough stock
//! ```
//!
//! No 409, because nothing here can conflict. What replaces it is `waited_ms`:
//! the time this request spent queued behind other writers. That number is the
//! strategy's bill, and unlike a conflict it never shows up as an error — which
//! is why a service like this degrades by getting slower and slower rather than
//! by failing, and why the first symptom in production is usually a client-side
//! timeout somewhere else entirely.
//!
//! `expected_version` is accepted and ignored here too, but for a different
//! reason than in naive-service: there is genuinely nothing for it to do. The
//! lock already guarantees what the version check would have proved.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::store::Store;

pub const STRATEGY: &str = "pessimistic";

#[derive(Deserialize)]
struct SeedBody {
    units: u64,
}

#[derive(Deserialize)]
struct ReserveBody {
    units: u64,
    /// Accepted for contract compatibility. The lock makes it redundant.
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
    waited_ms: u64,
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
        waited_ms = reservation.waited_ms,
        "reserved"
    );

    Ok(Json(ReserveView {
        strategy: STRATEGY,
        product,
        reserved: reservation.reserved,
        available: reservation.available,
        version: reservation.version,
        waited_ms: reservation.waited_ms,
    }))
}
