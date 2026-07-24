//! # Orders query service — the read side of the Orders bounded context.
//!
//! This deployable shares no code, no process, and no datastore with
//! orders-command-service. It knows nothing about the event-sourced
//! aggregate, the append log, or any business rule for what transitions are
//! legal — it only knows how to fold the four order lifecycle events into a
//! view and serve it back over HTTP. That's the whole service.

mod events;
mod http;
mod projection;
mod view;

use axum::{routing::get, Router};

use view::SharedOrderViews;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,orders_query_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let views = SharedOrderViews::default();
    projection::spawn(nats, views.clone()).await;
    tracing::info!("subscribed to orders.placed, orders.paid, orders.shipped, orders.cancelled");

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(views));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3004".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("orders-query-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
