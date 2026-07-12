//! Orders' inbound HTTP surface. Unchanged in spirit from the monolith: it maps
//! a request onto the service and returns JSON. The interesting collaboration now
//! happens *behind* the service, in the outbound HTTP clients (see clients.rs).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
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
) -> Result<Json<Order>, AppError> {
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
    Ok(Json(order))
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
