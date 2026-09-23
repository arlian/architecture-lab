//! # ledger-service — the consumer, keeping two sets of books at once.
//!
//! Every delivered event adds its amount to revenue. The service keeps the
//! books twice, from the same stream of deliveries:
//!
//! * `without_inbox` — apply every delivery. What a consumer does when it
//!   trusts the broker to deliver exactly once, which no broker does.
//! * `with_inbox`    — remember every `event_id` applied, and acknowledge but
//!   skip one already seen. That set is the inbox; with a real database it is
//!   a table written in the same transaction as the revenue.
//!
//! Keeping both side by side is the lab's shortcut: one run shows what the
//! duplicates would have cost and that the inbox absorbed them.
//!
//! ```text
//! POST /events         {event_id, source, order_id, amount} -> 200
//! GET  /books/:source  -> 200 { deliveries, without_inbox: {events, revenue},
//!                                           with_inbox:    {events, revenue} }
//! ```
//!
//! Knobs: `PORT` (3040).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

/// Duplicated from orders-service on purpose: the two sides share a message
/// format, not a crate.
#[derive(Deserialize)]
struct Event {
    event_id: String,
    source: String,
    #[allow(dead_code)]
    order_id: u64,
    amount: u64,
}

#[derive(Default, Clone, Serialize)]
struct Tally {
    events: u64,
    revenue: u64,
}

#[derive(Default, Clone, Serialize)]
struct Books {
    deliveries: u64,
    without_inbox: Tally,
    with_inbox: Tally,
}

#[derive(Default)]
struct Ledger {
    /// The inbox: every event id ever applied.
    seen: HashSet<String>,
    books: HashMap<String, Books>,
}

type Shared = Arc<Mutex<Ledger>>;

async fn receive(State(ledger): State<Shared>, Json(event): Json<Event>) {
    let mut ledger = ledger.lock().unwrap();
    let first_time = ledger.seen.insert(event.event_id.clone());
    let books = ledger.books.entry(event.source).or_default();

    books.deliveries += 1;
    books.without_inbox.events += 1;
    books.without_inbox.revenue += event.amount;

    if first_time {
        books.with_inbox.events += 1;
        books.with_inbox.revenue += event.amount;
    } else {
        // Still a 200: the producer must stop resending. A redelivery is not
        // an error, it is the protocol working.
        tracing::debug!("inbox: skipped duplicate {}", event.event_id);
    }
}

async fn books(State(ledger): State<Shared>, Path(source): Path<String>) -> Json<Books> {
    let ledger = ledger.lock().unwrap();
    Json(ledger.books.get(&source).cloned().unwrap_or_default())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/events", post(receive))
        .route("/books/:source", get(books))
        .with_state(Shared::default());

    let port = std::env::var("PORT").unwrap_or_else(|_| "3040".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("ledger-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
