//! Driving adapter #1: HTTP.
//!
//! **Read this file next to `console.rs`.** They drive the same three use
//! cases and share nothing else.
//!
//! Everything here is translation, in both directions:
//!
//! * inbound — a JSON body and a path segment become a `PlaceOrder` command
//!   and an `OrderId`. A malformed UUID is an HTTP concern, and it never
//!   reaches the core;
//! * outbound — an `Order` becomes JSON, and a `DomainError` becomes a status
//!   code.
//!
//! ## The orphan rule does the architecture's arguing for you
//!
//! In `microservices/`, `AppError` had `impl IntoResponse` right there in
//! `error.rs`, sat next to the domain — a small, harmless-looking line that
//! made "not found" and "404" the same idea, and quietly meant the domain
//! could only ever live inside a web server.
//!
//! Try writing that impl here and Rust refuses: `DomainError` is a foreign
//! type and `IntoResponse` is a foreign trait, so the impl is illegal in both
//! crates. The only legal home for it is a local newtype in the adapter —
//! [`ApiError`] below. The compiler will not let the web framework's
//! vocabulary into the hexagon even if you want it there.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use orders_core::{DomainError, Order, OrderId, OrderService, PlaceOrder, PlaceOrderLine};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

/// The HTTP adapter's private view of a domain failure.
struct ApiError(DomainError);

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // The one place in the entire workspace that believes in status codes.
        let status = match &self.0 {
            DomainError::NotFound(_) => StatusCode::NOT_FOUND,
            DomainError::Validation(_) => StatusCode::BAD_REQUEST,
            // 503, not 500: the core told us a dependency was unavailable, so
            // we can say something more useful than "something broke".
            DomainError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}

/// Wire format, owned by the adapter. The core's `PlaceOrder` is a Rust
/// struct with newtype ids; this is whatever shape we've promised HTTP
/// clients. Keeping them separate is what lets the API version independently
/// of the domain — rename a JSON field here and the core never notices.
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
        .route("/health", get(|| async { "ok" }))
        .route("/orders", post(place).get(list))
        .route("/orders/:id", get(get_one))
        .with_state(service)
}

async fn place(
    State(svc): State<Arc<OrderService>>,
    Json(req): Json<PlaceOrderRequest>,
) -> Result<Json<Order>, ApiError> {
    let cmd = PlaceOrder {
        user_id: orders_core::UserId(req.user_id),
        lines: req
            .lines
            .into_iter()
            .map(|l| PlaceOrderLine {
                product_id: orders_core::ProductId(l.product_id),
                quantity: l.quantity,
            })
            .collect(),
    };
    Ok(Json(svc.place(cmd).await?))
}

async fn get_one(
    State(svc): State<Arc<OrderService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Order>, ApiError> {
    Ok(Json(svc.get(OrderId(id)).await?))
}

async fn list(State(svc): State<Arc<OrderService>>) -> Result<Json<Vec<Order>>, ApiError> {
    Ok(Json(svc.list().await?))
}
