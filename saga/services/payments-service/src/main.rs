//! # Payments service — a saga participant, new in this lab.
//!
//! Owns wallet balances and does exactly two things: opens one from Users'
//! `UserRegistered` events (seed.rs), and debits it in response to a
//! directed saga command from orders-service, the orchestrator (reactor.rs).
//! Like every service in this lab it never calls another service directly —
//! it only publishes/subscribes through NATS.

mod bus;
mod domain;
mod error;
mod events;
mod http;
mod reactor;
mod repository;
mod seed;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use bus::NatsBus;
use repository::InMemoryPaymentsRepository;
use service::PaymentsService;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,payments_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let repo = Arc::new(InMemoryPaymentsRepository::default());
    let events = Arc::new(NatsBus::new(nats.clone()));
    let service = Arc::new(PaymentsService::new(repo, events));

    seed::spawn(nats.clone(), service.clone()).await;
    reactor::spawn(nats, service.clone()).await;
    tracing::info!("subscribed to users.registered, payments.charge.requested");

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3005".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("payments-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
