//! The whole lab, in two functions. Same input, same order row, same event;
//! the only difference is *when* the event leaves, and what it is written with.

use crate::db::OutboxRow;
use crate::AppState;

/// Commit, then publish. Two writes, two systems, no transaction spanning
/// them — so there is a moment where the first has happened and the second
/// has not, and a crash in that moment is an order the ledger never hears of.
///
/// Note the answer the client gets: a 500 for an order that *was* placed. If
/// it retries, there are now two orders. Dual writes lie in both directions.
pub async fn place_naive(state: &AppState, amount: u64) -> Result<u64, String> {
    let id = state.db.transaction(|t| t.insert_order(amount));

    if state.crashes() {
        return Err(format!("crashed after committing order {id}, before publishing"));
    }

    state
        .publisher
        .send(&state.event(id, amount))
        .await
        .map_err(|e| format!("order {id} committed, publish failed: {e}"))?;
    Ok(id)
}

/// Commit the order and the event together; publish nothing. One write, one
/// system, one transaction. The gap is gone because the network call has left
/// the request path entirely; `relay.rs` takes it from here.
pub fn place_outbox(state: &AppState, amount: u64) -> u64 {
    state.db.transaction(|t| {
        let id = t.insert_order(amount);
        t.outbox.push(OutboxRow {
            event: state.event(id, amount),
            sent: false,
        });
        id
    })
}
