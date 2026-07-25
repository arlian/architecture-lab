//! Inventory's read-only HTTP surface — for inspecting the effect of a
//! reservation/release in the demo walkthrough. There is deliberately no
//! POST here: stock is never set directly over HTTP, only seeded from
//! `catalog.product_created` and moved by saga commands over NATS.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::ProductId;
use crate::error::AppError;
use crate::service::InventoryService;

#[derive(Serialize)]
struct StockResponse {
    product_id: Uuid,
    available_units: u32,
}

pub fn router(service: Arc<InventoryService>) -> Router {
    Router::new()
        .route("/stock/:product_id", get(get_stock))
        .with_state(service)
}

async fn get_stock(
    State(svc): State<Arc<InventoryService>>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<StockResponse>, AppError> {
    let available_units = svc
        .available(ProductId(product_id))
        .await
        .ok_or_else(|| AppError::NotFound(format!("product {product_id}")))?;
    Ok(Json(StockResponse {
        product_id,
        available_units,
    }))
}
