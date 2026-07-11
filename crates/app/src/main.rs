//! # Composition root
//!
//! This is the ONLY place that knows about every module at once. Its job:
//!   1. construct each module,
//!   2. wire cross-module dependencies (Orders needs Users + Catalog),
//!   3. merge each module's routes into one HTTP surface,
//!   4. run the server.
//!
//! Because the wiring lives here — and nowhere else — modules stay ignorant of
//! each other's construction. Swap a module's internals, or add a new module,
//! and the changes stay local.

use std::sync::Arc;

use axum::{routing::get, Router};

use catalog::{CatalogModule, CreateProduct};
use orders::OrdersModule;
use users::UsersModule;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,app=debug".into()),
        )
        .init();

    // 1. Build the independent modules.
    let users = UsersModule::new();
    let catalog = CatalogModule::new();

    // 2. Wire Orders against the *public APIs* of Users and Catalog. The `as
    //    Arc<dyn ...>` coercions are where the concrete services become abstract
    //    contracts — the essence of the modular-monolith boundary.
    let orders = OrdersModule::new(
        users.service.clone() as Arc<dyn users::api::UserDirectory>,
        catalog.service.clone() as Arc<dyn catalog::api::ProductCatalog>,
    );

    // A little seed data so the API is usable immediately.
    seed_catalog(&catalog).await;

    // 3. Merge every module's routes into a single app.
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(users.router())
        .merge(catalog.router())
        .merge(orders.router());

    // 4. Serve.
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    tracing::info!("architecture-lab listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}

async fn seed_catalog(catalog: &CatalogModule) {
    for (name, price_cents) in [("Coffee Mug", 1299), ("Notebook", 850)] {
        let _ = catalog
            .service
            .create(CreateProduct {
                name: name.into(),
                price_cents,
            })
            .await;
    }
}
