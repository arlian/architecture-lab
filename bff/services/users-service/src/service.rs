//! Users use cases. Identical business rules to the monolith's Users module —
//! the logic doesn't change when you split into services; only how the service
//! is *reached* (network instead of a function call) does.

use std::sync::Arc;

use crate::domain::{User, UserId};
use crate::error::AppError;
use crate::repository::UserRepository;

pub struct CreateUser {
    pub email: String,
    pub name: String,
}

pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(&self, cmd: CreateUser) -> Result<User, AppError> {
        if cmd.email.trim().is_empty() || !cmd.email.contains('@') {
            return Err(AppError::Validation("a valid email is required".into()));
        }
        if cmd.name.trim().is_empty() {
            return Err(AppError::Validation("name is required".into()));
        }
        if self.repo.find_by_email(&cmd.email).await.is_some() {
            return Err(AppError::Conflict(format!(
                "email {} is already registered",
                cmd.email
            )));
        }

        let user = User {
            id: UserId::new(),
            email: cmd.email,
            name: cmd.name,
        };
        Ok(self.repo.insert(user).await)
    }

    pub async fn get(&self, id: UserId) -> Result<User, AppError> {
        self.repo
            .get(id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("user {id}")))
    }

    pub async fn list(&self) -> Vec<User> {
        self.repo.all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::InMemoryUserRepository;

    fn service() -> UserService {
        UserService::new(Arc::new(InMemoryUserRepository::default()))
    }

    #[tokio::test]
    async fn creates_and_reads_back_a_user() {
        let svc = service();
        let user = svc
            .create(CreateUser {
                email: "ada@example.com".into(),
                name: "Ada".into(),
            })
            .await
            .unwrap();

        let fetched = svc.get(user.id).await.unwrap();
        assert_eq!(fetched.email, "ada@example.com");
    }

    #[tokio::test]
    async fn rejects_duplicate_email() {
        let svc = service();
        let cmd = || CreateUser {
            email: "dup@example.com".into(),
            name: "Dup".into(),
        };
        svc.create(cmd()).await.unwrap();
        let err = svc.create(cmd()).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }
}
