//! # mobile-bff — the gateway tailored to the mobile client's order-status
//! screen.
//!
//! Same shape as web-bff (owns no data, just fans out and reshapes), but
//! notice what's absent: no `CATALOG_URL`, no catalog client anywhere in this
//! binary. This service's dependency graph is smaller because its client
//! needs less — that's a property of the BFF pattern itself, not an
//! optimization bolted on afterwards.

mod clients;
mod error;
mod http;
mod views;

use std::sync::Arc;

use axum::{routing::get, Router};

use clients::{OrdersHttpClient, UsersHttpClient};
use http::AppState;
use views::OrderAggregator;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,mobile_bff=debug".into()),
        )
        .init();

    let orders_url =
        std::env::var("ORDERS_URL").unwrap_or_else(|_| "http://localhost:3003".into());
    let users_url =
        std::env::var("USERS_URL").unwrap_or_else(|_| "http://localhost:3001".into());
    tracing::info!("mobile-bff will call orders at {orders_url}, users at {users_url}");

    let orders: Arc<dyn clients::OrdersClient> = Arc::new(OrdersHttpClient::new(orders_url));
    let users: Arc<dyn clients::UsersClient> = Arc::new(UsersHttpClient::new(users_url));
    let aggregator = Arc::new(OrderAggregator::new(orders.clone(), users));

    let state = Arc::new(AppState { orders, aggregator });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3005".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("mobile-bff listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
