//! Driven adapters for [`UserDirectory`] and [`ProductCatalog`] — two fixed
//! tables, hardcoded at startup.
//!
//! In `microservices/` these exact two ports were backed by `reqwest` and
//! pointed at `users-service:3001` and `catalog-service:3002`. Here they're a
//! `HashSet` and a `HashMap` compiled into the binary, and **the core did not
//! change by one character** — that's the claim, and this file is where you
//! can check it.
//!
//! It also makes the lab runnable on its own, with no other service to start.
//! Pointing these at the real HTTP services is one of the exercises: it's a
//! new file in this crate, and nothing else in the workspace moves.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use orders_core::{DomainError, ProductCatalog, ProductId, UserDirectory, UserId};
use uuid::Uuid;

/// Fixed ids so the README's `curl` and CLI examples are copy-pasteable.
pub const ADA: &str = "11111111-1111-1111-1111-111111111111";
pub const COFFEE_MUG: &str = "22222222-2222-2222-2222-222222222222";
pub const NOTEBOOK: &str = "33333333-3333-3333-3333-333333333333";

pub struct StaticUserDirectory {
    users: HashSet<UserId>,
}

#[async_trait]
impl UserDirectory for StaticUserDirectory {
    async fn exists(&self, id: UserId) -> Result<bool, DomainError> {
        Ok(self.users.contains(&id))
    }
}

pub struct StaticProductCatalog {
    /// id -> (name, price). The name is only here so the composition roots
    /// can print something friendly at startup; the core never asks for it,
    /// because pricing a line is all it needs.
    products: HashMap<ProductId, (String, u64)>,
}

impl StaticProductCatalog {
    pub fn listing(&self) -> Vec<(ProductId, String, u64)> {
        let mut items: Vec<_> = self
            .products
            .iter()
            .map(|(id, (name, price))| (*id, name.clone(), *price))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        items
    }
}

#[async_trait]
impl ProductCatalog for StaticProductCatalog {
    async fn price_of(&self, id: ProductId) -> Result<Option<u64>, DomainError> {
        Ok(self.products.get(&id).map(|(_, price)| *price))
    }
}

/// The same two products the catalog-service seeds in the other labs, so the
/// prices you see here match the ones over there.
pub fn seed() -> (StaticUserDirectory, StaticProductCatalog) {
    let users = StaticUserDirectory {
        users: HashSet::from([UserId(parse_id(ADA))]),
    };
    let catalog = StaticProductCatalog {
        products: HashMap::from([
            (ProductId(parse_id(COFFEE_MUG)), ("Coffee Mug".to_string(), 1299)),
            (ProductId(parse_id(NOTEBOOK)), ("Notebook".to_string(), 850)),
        ]),
    };
    (users, catalog)
}

fn parse_id(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("seed ids are valid UUIDs")
}
