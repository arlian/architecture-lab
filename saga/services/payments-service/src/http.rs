//! Payments' read-only HTTP surface — for inspecting a wallet's balance in
//! the demo walkthrough. There is deliberately no POST here: a balance is
//! never set directly over HTTP, only opened from `users.registered` and
//! debited by saga commands over NATS.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::UserId;
use crate::error::AppError;
use crate::service::PaymentsService;

#[derive(Serialize)]
struct WalletResponse {
    user_id: Uuid,
    balance_cents: u64,
}

pub fn router(service: Arc<PaymentsService>) -> Router {
    Router::new()
        .route("/wallets/:user_id", get(get_wallet))
        .with_state(service)
}

async fn get_wallet(
    State(svc): State<Arc<PaymentsService>>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<WalletResponse>, AppError> {
    let balance_cents = svc
        .balance(UserId(user_id))
        .await
        .ok_or_else(|| AppError::NotFound(format!("wallet for user {user_id}")))?;
    Ok(Json(WalletResponse {
        user_id,
        balance_cents,
    }))
}
