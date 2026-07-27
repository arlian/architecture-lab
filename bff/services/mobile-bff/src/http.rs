//! mobile-bff's HTTP surface: one thin proxy endpoint (`POST /checkout`) and
//! one aggregation endpoint (`GET /orders/:id`) — same two-endpoint shape as
//! web-bff, but `order_summary` does less work per request. See views.rs.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::clients::{OrderView, OrdersClient, PlaceOrderLineRequest, PlaceOrderRequest};
use crate::error::AppError;
use crate::views::{OrderAggregator, OrderSummaryView};

#[derive(Deserialize)]
struct CheckoutRequest {
    user_id: Uuid,
    lines: Vec<CheckoutLineRequest>,
}

#[derive(Deserialize)]
struct CheckoutLineRequest {
    product_id: Uuid,
    quantity: u32,
}

pub struct AppState {
    pub orders: Arc<dyn OrdersClient>,
    pub aggregator: Arc<OrderAggregator>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/checkout", post(checkout))
        .route("/orders/:id", get(order_summary))
        .with_state(state)
}

async fn checkout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<OrderView>, AppError> {
    let order = state
        .orders
        .place(PlaceOrderRequest {
            user_id: req.user_id,
            lines: req
                .lines
                .into_iter()
                .map(|l| PlaceOrderLineRequest {
                    product_id: l.product_id,
                    quantity: l.quantity,
                })
                .collect(),
        })
        .await?;
    Ok(Json(order))
}

async fn order_summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderSummaryView>, AppError> {
    let view = state.aggregator.order_summary(id).await?;
    Ok(Json(view))
}
