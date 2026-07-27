//! Users persistence. Each service owns its own datastore; here it's a simple
//! in-memory map so the service runs with zero infrastructure. The point that
//! matters for microservices: this data is private. No cross-service database
//! joins — the only way in is this service's HTTP API.

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
