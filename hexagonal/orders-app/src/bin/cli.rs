//! Composition root #2 — drive the same core from a shell.
//!
//! Compare with `serve.rs`. The wiring block is nearly identical, because the
//! core's needs don't change with who's calling; what changes is which
//! driving adapter runs afterwards, and how failure is reported (exit code
//! instead of status code).
//!
//! One deliberate difference in the wiring: this root always picks the file
//! repository. A CLI process lives for milliseconds, so an in-memory store
//! would forget the order before you got your prompt back. That is a
//! *composition* decision — the shape of the process demanded it — and it's
//! resolved here, at assembly time, without the core knowing that persistence
//! is even an option.

use std::sync::Arc;

use orders_app::{console, directory, repository::JsonFileOrderRepository};
use orders_core::OrderService;

#[tokio::main]
async fn main() {
    let path = std::env::var("ORDERS_FILE").unwrap_or_else(|_| "orders.json".into());
    let (users, catalog) = directory::seed();
    let service = Arc::new(OrderService::new(
        Arc::new(JsonFileOrderRepository::new(path)),
        Arc::new(users),
        Arc::new(catalog),
    ));

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = console::run(service, &args).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
