//! # orders-service — one service, two ways to make a second write.
//!
//! Placing an order is two writes: the order row, and a message telling the
//! ledger about it. They live in two systems, so no single transaction covers
//! both. `MODE` picks how that gap is handled:
//!
//! * `naive`  — commit the order, then publish. A crash in between loses the
//!   event forever, and nothing ever notices.
//! * `outbox` — commit the order *and* the event in one transaction; a relay
//!   publishes later. Nothing is lost, but a crash after publishing and before
//!   marking the row sent means the event goes out twice.
//!
//! Knobs: `MODE` (naive), `CRASH_RATE` (0.1), `LEDGER_URL`
//! (http://localhost:3040/events), `PORT` (3030).
//!
//! Read `write.rs` first: both strategies sit next to each other there.

mod db;
mod http;
mod publish;
mod relay;
mod write;

use std::sync::Arc;

use db::{Db, Event};
use publish::Publisher;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Naive,
    Outbox,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Naive => "naive",
            Mode::Outbox => "outbox",
        }
    }
}

pub struct AppState {
    pub mode: Mode,
    pub db: Db,
    pub publisher: Publisher,
    crash_rate: f64,
    /// Order ids restart at 1 with the process; the nonce keeps event ids
    /// unique across restarts, so the ledger's inbox never confuses two runs.
    nonce: u32,
}

impl AppState {
    /// A simulated crash. True with probability `CRASH_RATE`; the caller stops
    /// dead exactly where a real process would.
    pub fn crashes(&self) -> bool {
        rand::random::<f64>() < self.crash_rate
    }

    pub fn event(&self, order_id: u64, amount: u64) -> Event {
        Event {
            event_id: format!("{}-{:08x}-{order_id}", self.mode.name(), self.nonce),
            source: self.mode.name().to_string(),
            order_id,
            amount,
        }
    }
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,orders_service=debug".into()),
        )
        .init();

    let mode = match std::env::var("MODE").as_deref() {
        Ok("outbox") => Mode::Outbox,
        Ok("naive") | Err(_) => Mode::Naive,
        Ok(other) => panic!("MODE must be `naive` or `outbox`, got {other:?}"),
    };
    let crash_rate: f64 = env_or("CRASH_RATE", 0.1);
    let ledger_url: String = env_or("LEDGER_URL", "http://localhost:3040/events".to_string());
    let port: u16 = env_or("PORT", 3030);

    let state = Arc::new(AppState {
        mode,
        db: Db::default(),
        publisher: Publisher::new(ledger_url),
        crash_rate,
        nonce: rand::random(),
    });

    if mode == Mode::Outbox {
        tokio::spawn(relay::run(state.clone()));
    }

    let app = http::router(state);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!(
        "orders-service listening on http://{addr} (mode: {}, crash rate {crash_rate})",
        mode.name()
    );
    axum::serve(listener, app).await.expect("server error");
}
