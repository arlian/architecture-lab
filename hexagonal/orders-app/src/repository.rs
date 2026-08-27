//! Driven adapters for [`OrderRepository`] — two of them.
//!
//! This file is the payoff. Two completely different storage strategies, one
//! of which can genuinely fail at runtime, and `orders-core` contains not a
//! single line that knows which one it got. Swapping them is one `match` arm
//! in `bin/serve.rs`.
//!
//! Note what each adapter has to do at its edges: [`JsonFileOrderRepository`]
//! deals in paths, bytes, and parse errors, and translates every one of those
//! failures into `DomainError::Unavailable` before it crosses the wall. Ugly
//! infrastructure detail stops here, at the port, which is precisely its job.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use orders_core::{DomainError, Order, OrderId, OrderRepository};
use tokio::sync::{Mutex, RwLock};

/// Orders live in a `HashMap` and die with the process.
#[derive(Default)]
pub struct InMemoryOrderRepository {
    inner: RwLock<HashMap<OrderId, Order>>,
}

#[async_trait]
impl OrderRepository for InMemoryOrderRepository {
    async fn insert(&self, order: Order) -> Result<Order, DomainError> {
        self.inner.write().await.insert(order.id, order.clone());
        Ok(order)
    }

    async fn get(&self, id: OrderId) -> Result<Option<Order>, DomainError> {
        Ok(self.inner.read().await.get(&id).cloned())
    }

    async fn all(&self) -> Result<Vec<Order>, DomainError> {
        Ok(self.inner.read().await.values().cloned().collect())
    }
}

/// Orders live in a JSON file and survive a restart.
///
/// Deliberately the dumbest durable thing that works: read the whole file,
/// mutate, write the whole file back, with a mutex so two concurrent requests
/// can't interleave. It is not a database and makes no attempt to be one —
/// what matters for this lab is that it's *outside*, it's *fallible*, and the
/// core is indifferent to both facts.
pub struct JsonFileOrderRepository {
    path: PathBuf,
    /// Guards the read-modify-write cycle. A real database would push this
    /// concern down into the storage engine; here the adapter owns it, which
    /// is the right place for it either way — the core should never be asked
    /// to think about file locking.
    write_lock: Mutex<()>,
}

impl JsonFileOrderRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    /// A missing file is an empty store, not an error — that's this adapter's
    /// decision to make, and the core never hears about it.
    async fn load(&self) -> Result<Vec<Order>, DomainError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                DomainError::Unavailable(format!("{} is not valid order JSON: {e}", self.display()))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(DomainError::Unavailable(format!(
                "cannot read {}: {e}",
                self.display()
            ))),
        }
    }

    async fn store(&self, orders: &[Order]) -> Result<(), DomainError> {
        let bytes = serde_json::to_vec_pretty(orders)
            .map_err(|e| DomainError::Unavailable(format!("cannot encode orders: {e}")))?;
        tokio::fs::write(&self.path, bytes)
            .await
            .map_err(|e| DomainError::Unavailable(format!("cannot write {}: {e}", self.display())))
    }

    fn display(&self) -> String {
        self.path.display().to_string()
    }
}

#[async_trait]
impl OrderRepository for JsonFileOrderRepository {
    async fn insert(&self, order: Order) -> Result<Order, DomainError> {
        let _guard = self.write_lock.lock().await;
        let mut orders = self.load().await?;
        orders.push(order.clone());
        self.store(&orders).await?;
        Ok(order)
    }

    async fn get(&self, id: OrderId) -> Result<Option<Order>, DomainError> {
        Ok(self.load().await?.into_iter().find(|o| o.id == id))
    }

    async fn all(&self) -> Result<Vec<Order>, DomainError> {
        self.load().await
    }
}
