//! Users persistence. Private, in-memory, owned by this service alone — same
//! as the microservices version. Going event-driven changes how OTHER services
//! learn about users; it changes nothing about who owns the users table.

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
