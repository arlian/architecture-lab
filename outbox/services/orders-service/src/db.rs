//! The database, in memory. One mutex guards both tables, so anything done
//! while holding it is a transaction in the only sense this lab needs: other
//! callers see all of it or none of it. Swap in Postgres and `transaction`
//! becomes `BEGIN ... COMMIT`; nothing else in the service changes.

use std::sync::Mutex;

use serde::Serialize;

/// What the ledger is told. `event_id` is the idempotency key the inbox needs.
#[derive(Clone, Serialize)]
pub struct Event {
    pub event_id: String,
    pub source: String,
    pub order_id: u64,
    pub amount: u64,
}

pub struct OutboxRow {
    pub event: Event,
    pub sent: bool,
}

#[derive(Default)]
pub struct Tables {
    /// (order_id, amount)
    pub orders: Vec<(u64, u64)>,
    pub outbox: Vec<OutboxRow>,
}

impl Tables {
    pub fn insert_order(&mut self, amount: u64) -> u64 {
        let id = self.orders.len() as u64 + 1;
        self.orders.push((id, amount));
        id
    }
}

#[derive(Default)]
pub struct Db {
    tables: Mutex<Tables>,
}

pub struct Stats {
    pub orders: u64,
    pub revenue: u64,
    pub pending_outbox: u64,
}

impl Db {
    /// Run `f` atomically against every table. Never hold this across an
    /// `.await`: a transaction that waits on the network is the very thing the
    /// outbox exists to avoid.
    pub fn transaction<R>(&self, f: impl FnOnce(&mut Tables) -> R) -> R {
        f(&mut self.tables.lock().unwrap())
    }

    /// Outbox rows not yet published, oldest first, with their row index.
    pub fn unsent(&self) -> Vec<(usize, Event)> {
        self.transaction(|t| {
            t.outbox
                .iter()
                .enumerate()
                .filter(|(_, row)| !row.sent)
                .map(|(i, row)| (i, row.event.clone()))
                .collect()
        })
    }

    pub fn mark_sent(&self, row: usize) {
        self.transaction(|t| t.outbox[row].sent = true);
    }

    pub fn stats(&self) -> Stats {
        self.transaction(|t| Stats {
            orders: t.orders.len() as u64,
            revenue: t.orders.iter().map(|(_, amount)| amount).sum(),
            pending_outbox: t.outbox.iter().filter(|row| !row.sent).count() as u64,
        })
    }
}
