//! The outbox relay: poll unsent rows, publish, mark sent.
//!
//! Publish and mark-sent are, again, two writes to two systems. The outbox did
//! not remove the dual write, it *moved* it somewhere retrying is safe: a row
//! that is never marked just goes out again next tick. That is at-least-once
//! delivery — nothing lost, duplicates possible — and it is why the consumer
//! needs an inbox (see ledger-service).

use std::sync::Arc;
use std::time::Duration;

use crate::AppState;

pub async fn run(state: Arc<AppState>) {
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    loop {
        tick.tick().await;
        for (row, event) in state.db.unsent() {
            if let Err(e) = state.publisher.send(&event).await {
                // Ledger unreachable. Stop, and retry next tick in order.
                tracing::debug!("relay: publish failed, will retry: {e}");
                break;
            }
            if state.crashes() {
                // Published, never marked. The next tick sends it again.
                tracing::debug!("relay: crashed after publishing {}", event.event_id);
                break;
            }
            state.db.mark_sent(row);
        }
    }
}
