//! The same contract a fourth time:
//!
//! ```text
//! GET  /stock/:product                  -> 200 { strategy, product, available, version }
//! POST /stock/:product/seed    {units}  -> 200 { strategy, product, available, version }
//! POST /stock/:product/reserve {units, expected_version?}
//!                                       -> 200 { strategy, product, reserved, available, version, queued_ms }
//!                                       -> 404 unknown product
//!                                       -> 422 not enough stock
//!                                       -> 503 the writer is saturated
//! ```
//!
//! Two things are different here and both are structural rather than cosmetic.
//!
//! **The state is a `StockActor`, not an `Arc<Store>`.** Every other service in
//! this lab hands its handlers a shared reference to the data. This one hands
//! them a *channel*, which is `Clone` and holds nothing. There is no shared
//! reference to give out, so no handler — including one written next year by
//! someone who has not read `actor.rs` — can reach the `HashMap` and forget to
//! coordinate. The type system, not a convention, is what stops it.
//!
//! **There is a 503.** This is the only service here that can decline work. The
//! other three accept everything and express overload as latency, which is the
//! same thing said in a language nobody monitors.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::actor::StockActor;
use crate::error::{AppError, ReserveError};

pub const STRATEGY: &str = "actor";

#[derive(Deserialize)]
struct SeedBody {
    units: u64,
}

#[derive(Deserialize)]
struct ReserveBody {
    units: u64,
    /// Accepted for contract compatibility. Serialised ownership makes it
    /// redundant, the same way the lock does in pessimistic-service.
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
    queued_ms: u64,
}

pub fn router(actor: StockActor) -> Router {
    Router::new()
        .route("/stock/:product", get(read_stock))
        .route("/stock/:product/seed", post(seed))
        .route("/stock/:product/reserve", post(reserve))
        .with_state(actor)
}

async fn read_stock(
    State(actor): State<StockActor>,
    Path(product): Path<String>,
) -> Result<Json<StockView>, AppError> {
    let entry = actor
        .get(&product)
        .await?
        .ok_or_else(|| ReserveError::UnknownProduct(product.clone()))?;

    Ok(Json(StockView {
        strategy: STRATEGY,
        product,
        available: entry.units,
        version: entry.version,
    }))
}

async fn seed(
    State(actor): State<StockActor>,
    Path(product): Path<String>,
    Json(body): Json<SeedBody>,
) -> Result<Json<StockView>, AppError> {
    let entry = actor.seed(&product, body.units).await?;
    tracing::info!(product = %product, units = body.units, "seeded");

    Ok(Json(StockView {
        strategy: STRATEGY,
        product,
        available: entry.units,
        version: entry.version,
    }))
}

async fn reserve(
    State(actor): State<StockActor>,
    Path(product): Path<String>,
    Json(body): Json<ReserveBody>,
) -> Result<Json<ReserveView>, AppError> {
    if body.units == 0 {
        return Err(AppError::Validation("`units` must be greater than zero".into()));
    }

    let reservation = actor.reserve(&product, body.units).await?;
    tracing::debug!(
        product = %product,
        reserved = reservation.reserved,
        available = reservation.available,
        queued_ms = reservation.queued_ms,
        "reserved"
    );

    Ok(Json(ReserveView {
        strategy: STRATEGY,
        product,
        reserved: reservation.reserved,
        available: reservation.available,
        version: reservation.version,
        queued_ms: reservation.queued_ms,
    }))
}
