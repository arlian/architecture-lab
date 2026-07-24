//! Orders' write-side HTTP surface. Deliberately narrow: there is no `GET
//! /orders` or `GET /orders/:id` here returning a normal order view — that's
//! orders-query-service's job (see its http.rs). The only read this service
//! exposes is `GET /orders/:id/history`, the raw event log, which is really
//! part of the write model's own audit trail rather than a business query.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::aggregate::OrderStatus;
use crate::domain::OrderId;
use crate::error::AppError;
use crate::service::{OrderCommandService, OrderView, PlaceOrder, PlaceOrderLine};

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

#[derive(Serialize)]
struct OrderLineResponse {
    product_id: Uuid,
    quantity: u32,
    unit_price_cents: u64,
}

#[derive(Serialize)]
struct OrderResponse {
    id: Uuid,
    user_id: Uuid,
    lines: Vec<OrderLineResponse>,
    total_cents: u64,
    status: OrderStatus,
}

impl From<OrderView> for OrderResponse {
    fn from(view: OrderView) -> Self {
        OrderResponse {
            id: view.id.0,
            user_id: view.state.user_id,
            lines: view
                .state
                .lines
                .iter()
                .map(|l| OrderLineResponse {
                    product_id: l.product_id,
                    quantity: l.quantity,
                    unit_price_cents: l.unit_price_cents,
                })
                .collect(),
            total_cents: view.state.total_cents,
            status: view.state.status,
        }
    }
}

pub fn router(service: Arc<OrderCommandService>) -> Router {
    Router::new()
        .route("/orders", post(place))
        .route("/orders/:id/pay", post(pay))
        .route("/orders/:id/ship", post(ship))
        .route("/orders/:id/cancel", post(cancel))
        .route("/orders/:id/history", get(history))
        .with_state(service)
}

async fn place(
    State(svc): State<Arc<OrderCommandService>>,
    Json(req): Json<PlaceOrderRequest>,
) -> Result<Json<OrderResponse>, AppError> {
    let cmd = PlaceOrder {
        user_id: req.user_id,
        lines: req
            .lines
            .into_iter()
            .map(|l| PlaceOrderLine {
                product_id: l.product_id,
                quantity: l.quantity,
            })
            .collect(),
    };
    let view = svc.place(cmd).await?;
    Ok(Json(view.into()))
}

async fn pay(
    State(svc): State<Arc<OrderCommandService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderResponse>, AppError> {
    let view = svc.pay(OrderId(id)).await?;
    Ok(Json(view.into()))
}

async fn ship(
    State(svc): State<Arc<OrderCommandService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderResponse>, AppError> {
    let view = svc.ship(OrderId(id)).await?;
    Ok(Json(view.into()))
}

async fn cancel(
    State(svc): State<Arc<OrderCommandService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderResponse>, AppError> {
    let view = svc.cancel(OrderId(id)).await?;
    Ok(Json(view.into()))
}

async fn history(
    State(svc): State<Arc<OrderCommandService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::aggregate::OrderEvent>>, AppError> {
    let events = svc.history(OrderId(id)).await?;
    Ok(Json(events))
}
