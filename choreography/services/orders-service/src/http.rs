//! Orders' inbound HTTP surface. `POST /orders` answers `202 Accepted`, same
//! as the saga lab and for the same reason: the order is `Pending` when this
//! handler returns, and callers poll `GET /orders/:id` to watch it settle.
//!
//! One thing to be honest about, though. In `saga/`, `202` meant "we have
//! started a workflow and we will drive it to completion." Here it means
//! "we have written this down and said so out loud." Those are different
//! promises, and this endpoint cannot tell the difference — which is exactly
//! why the status this endpoint reports is only as good as tracker.rs's
//! guesswork.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{Order, OrderId, ProductId, UserId};
use crate::error::AppError;
use crate::service::{OrderService, PlaceOrder, PlaceOrderLine};

#[derive(Deserialize)]
struct PlaceOrderRequest {
    user_id: Uuid,
    lines: Vec<PlaceOrderLineRequest>,
}

#[derive(Deserialize)]
struct PlaceOrderLineRequest {
    product_id: Uuid,
    quantity: u32,
}

pub fn router(service: Arc<OrderService>) -> Router {
    Router::new()
        .route("/orders", post(place).get(list))
        .route("/orders/:id", get(get_one))
        .with_state(service)
}

async fn place(
    State(svc): State<Arc<OrderService>>,
    Json(req): Json<PlaceOrderRequest>,
) -> Result<(StatusCode, Json<Order>), AppError> {
    let cmd = PlaceOrder {
        user_id: UserId(req.user_id),
        lines: req
            .lines
            .into_iter()
            .map(|l| PlaceOrderLine {
                product_id: ProductId(l.product_id),
                quantity: l.quantity,
            })
            .collect(),
    };
    let order = svc.place(cmd).await?;
    Ok((StatusCode::ACCEPTED, Json(order)))
}

async fn get_one(
    State(svc): State<Arc<OrderService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Order>, AppError> {
    let order = svc.get(OrderId(id)).await?;
    Ok(Json(order))
}

async fn list(State(svc): State<Arc<OrderService>>) -> Json<Vec<Order>> {
    Json(svc.list().await)
}
