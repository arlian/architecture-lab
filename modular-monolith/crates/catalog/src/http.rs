//! HTTP adapter for the Catalog module.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shared::AppError;

use crate::domain::{Product, ProductId};
use crate::service::{CreateProduct, ProductService};

#[derive(Deserialize)]
struct CreateProductRequest {
    name: String,
    price_cents: u64,
}

pub(crate) fn router(service: Arc<ProductService>) -> Router {
    Router::new()
        .route("/products", post(create).get(list))
        .route("/products/:id", get(get_one))
        .with_state(service)
}

async fn create(
    State(svc): State<Arc<ProductService>>,
    Json(req): Json<CreateProductRequest>,
) -> Result<Json<Product>, AppError> {
    let product = svc
        .create(CreateProduct {
            name: req.name,
            price_cents: req.price_cents,
        })
        .await?;
    Ok(Json(product))
}

async fn get_one(
    State(svc): State<Arc<ProductService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Product>, AppError> {
    let product = svc.get(ProductId(id)).await?;
    Ok(Json(product))
}

async fn list(State(svc): State<Arc<ProductService>>) -> Json<Vec<Product>> {
    Json(svc.list().await)
}
