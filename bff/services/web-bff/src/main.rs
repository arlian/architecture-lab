//! # web-bff — the gateway tailored to the web client's order-detail screen.
//!
//! Like every other service in this lab, this binary is the whole deployable:
//! it wires its own HTTP clients, exposes its HTTP API, and runs on its own
//! port. It owns no data of its own — no repository, no domain model — it
//! exists purely to fan out to orders/users/catalog and reshape the result
//! for one specific frontend. Compare with mobile-bff/src/main.rs: same
//! shape, one fewer backend client wired up.

mod clients;
mod error;
mod http;
mod views;

use std::sync::Arc;

use axum::{routing::get, Router};

use clients::{CatalogHttpClient, OrdersHttpClient, UsersHttpClient};
use http::AppState;
use views::OrderAggregator;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,web_bff=debug".into()),
        )
        .init();

    let orders_url =
        std::env::var("ORDERS_URL").unwrap_or_else(|_| "http://localhost:3003".into());
    let users_url =
        std::env::var("USERS_URL").unwrap_or_else(|_| "http://localhost:3001".into());
    let catalog_url =
        std::env::var("CATALOG_URL").unwrap_or_else(|_| "http://localhost:3002".into());
    tracing::info!(
        "web-bff will call orders at {orders_url}, users at {users_url}, catalog at {catalog_url}"
    );

    let orders: Arc<dyn clients::OrdersClient> = Arc::new(OrdersHttpClient::new(orders_url));
    let users: Arc<dyn clients::UsersClient> = Arc::new(UsersHttpClient::new(users_url));
    let catalog: Arc<dyn clients::CatalogClient> = Arc::new(CatalogHttpClient::new(catalog_url));
    let aggregator = Arc::new(OrderAggregator::new(orders.clone(), users, catalog));

    let state = Arc::new(AppState { orders, aggregator });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3004".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("web-bff listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
