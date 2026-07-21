//! Catalog's HTTP surface, kept for direct human/admin use. Note that Orders
//! no longer calls `GET /products/:id` to price an order line — it reacts to
//! `ProductCreated` / `ProductPriceChanged` instead (see events.rs).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{Product, ProductId};
use crate::error::AppError;
use crate::service::{CreateProduct, ProductService};

#[derive(Deserialize)]
struct CreateProductRequest {
    name: String,
    price_cents: u64,
}

#[derive(Deserialize)]
struct UpdatePriceRequest {
    price_cents: u64,
}

pub fn router(service: Arc<ProductService>) -> Router {
    Router::new()
        .route("/products", post(create).get(list))
        .route("/products/:id", get(get_one))
        .route("/products/:id/price", put(update_price))
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

async fn update_price(
    State(svc): State<Arc<ProductService>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePriceRequest>,
) -> Result<Json<Product>, AppError> {
    let product = svc.update_price(ProductId(id), req.price_cents).await?;
    Ok(Json(product))
}

async fn list(State(svc): State<Arc<ProductService>>) -> Json<Vec<Product>> {
    Json(svc.list().await)
}
