//! The module's **public cross-module contract**.
//!
//! This is the ONLY surface other modules are allowed to depend on. By talking
//! to `UserDirectory` instead of `UserService` directly, the Orders module
//! stays decoupled from how Users is implemented — and can be tested against a
//! fake. Think of this trait as the module's published, stable interface.

use async_trait::async_trait;

use crate::domain::UserId;

#[async_trait]
pub trait UserDirectory: Send + Sync {
    /// Does a user with this id exist? Enough for other modules to enforce
    /// referential integrity without reaching into the Users store.
    async fn exists(&self, id: UserId) -> bool;
}

/// The real implementation is the service. Callers receive it as a trait object
/// (`Arc<dyn UserDirectory>`), so they never see `UserService`.
#[async_trait]
impl UserDirectory for crate::service::UserService {
    async fn exists(&self, id: UserId) -> bool {
        self.get(id).await.is_ok()
    }
}
