//! The persistence layer. The rest of the module depends on the
//! `UserRepository` trait, never on a concrete storage engine — so swapping the
//! in-memory store for Postgres later touches only this file.
//!
//! Everything here is `pub(crate)`: storage is an implementation detail and must
//! never leak outside the module.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::domain::{User, UserId};

#[async_trait]
pub(crate) trait UserRepository: Send + Sync {
    async fn insert(&self, user: User) -> User;
    async fn get(&self, id: UserId) -> Option<User>;
    async fn find_by_email(&self, email: &str) -> Option<User>;
    async fn all(&self) -> Vec<User>;
}

/// A simple in-memory implementation. Good enough to run the whole app with zero
/// infrastructure; replace it with a DB-backed repo without changing any caller.
#[derive(Default)]
pub(crate) struct InMemoryUserRepository {
    inner: RwLock<HashMap<UserId, User>>,
}

#[async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn insert(&self, user: User) -> User {
        self.inner.write().await.insert(user.id, user.clone());
        user
    }

    async fn get(&self, id: UserId) -> Option<User> {
        self.inner.read().await.get(&id).cloned()
    }

    async fn find_by_email(&self, email: &str) -> Option<User> {
        self.inner
            .read()
            .await
            .values()
            .find(|u| u.email == email)
            .cloned()
    }

    async fn all(&self) -> Vec<User> {
        self.inner.read().await.values().cloned().collect()
    }
}
