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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(order_id: u64, amount: u64) -> Event {
        Event {
            event_id: format!("evt-{order_id}"),
            source: "test".into(),
            order_id,
            amount,
        }
    }

    /// An order and its event committed in one transaction. Until the relay
    /// marks the row sent, the event is owed, and `stats` has to say so.
    #[test]
    fn an_order_and_its_event_land_together() {
        let db = Db::default();

        let id = db.transaction(|t| {
            let id = t.insert_order(40);
            t.outbox.push(OutboxRow {
                event: event(id, 40),
                sent: false,
            });
            id
        });

        let stats = db.stats();
        assert_eq!((stats.orders, stats.revenue, stats.pending_outbox), (1, 40, 1));
        let unsent = db.unsent();
        assert_eq!(unsent.len(), 1);
        assert_eq!(unsent[0].1.order_id, id);
    }

    /// The relay publishes oldest first and marks each row as it goes. A row
    /// marked sent must drop out of `unsent` and stop counting as pending,
    /// or the relay would publish it forever.
    #[test]
    fn marking_a_row_sent_removes_it_from_the_backlog() {
        let db = Db::default();
        db.transaction(|t| {
            for amount in [10, 20, 30] {
                let id = t.insert_order(amount);
                t.outbox.push(OutboxRow {
                    event: event(id, amount),
                    sent: false,
                });
            }
        });

        let first = db.unsent()[0].0;
        db.mark_sent(first);

        let left: Vec<u64> = db.unsent().iter().map(|(_, e)| e.order_id).collect();
        assert_eq!(left, vec![2, 3]);
        assert_eq!(db.stats().pending_outbox, 2);
        assert_eq!(db.stats().revenue, 60);
    }
}
