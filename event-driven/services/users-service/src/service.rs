//! Users use cases. Identical business rules to the microservices version.
//! The one addition: after a successful create, publish `UserRegistered` so
//! anyone downstream (today: orders-service's read model) can react.

use std::sync::Arc;

use crate::bus::EventBus;
use crate::domain::{User, UserId};
use crate::error::AppError;
use crate::events::UserRegistered;
use crate::repository::UserRepository;

pub struct CreateUser {
    pub email: String,
    pub name: String,
}

pub struct UserService {
    repo: Arc<dyn UserRepository>,
    events: Arc<dyn EventBus>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repo, events }
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
        let user = self.repo.insert(user).await;

        // Fire-and-forget: the user is already durably created in our own
        // store, so a broker hiccup here shouldn't fail the HTTP request. It
        // does mean a lost publish leaves downstream read models silently out
        // of sync — a real system would close that gap with a transactional
        // outbox. See the README's "where to take it next" for that exercise.
        let event = UserRegistered {
            id: user.id.0,
            email: user.email.clone(),
            name: user.name.clone(),
        };
        let payload = serde_json::to_vec(&event).expect("UserRegistered is serializable");
        if let Err(e) = self.events.publish(UserRegistered::SUBJECT, payload).await {
            tracing::warn!("failed to publish UserRegistered for {}: {e}", user.id);
        }

        Ok(user)
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
    use tokio::sync::Mutex;

    use crate::repository::InMemoryUserRepository;

    // A fake bus that records what was published, so tests can assert an
    // event went out without ever touching a socket — the same seam that let
    // the microservices lab's Orders tests use in-memory fakes.
    #[derive(Default)]
    struct FakeBus {
        published: Mutex<Vec<(&'static str, Vec<u8>)>>,
    }

    #[async_trait::async_trait]
    impl EventBus for FakeBus {
        async fn publish(&self, subject: &'static str, payload: Vec<u8>) -> Result<(), AppError> {
            self.published.lock().await.push((subject, payload));
            Ok(())
        }
    }

    fn service() -> (UserService, Arc<FakeBus>) {
        let bus = Arc::new(FakeBus::default());
        let svc = UserService::new(Arc::new(InMemoryUserRepository::default()), bus.clone());
        (svc, bus)
    }

    #[tokio::test]
    async fn creates_and_reads_back_a_user() {
        let (svc, _bus) = service();
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
        let (svc, _bus) = service();
        let cmd = || CreateUser {
            email: "dup@example.com".into(),
            name: "Dup".into(),
        };
        svc.create(cmd()).await.unwrap();
        let err = svc.create(cmd()).await.unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn publishes_user_registered_on_create() {
        let (svc, bus) = service();
        svc.create(CreateUser {
            email: "ada@example.com".into(),
            name: "Ada".into(),
        })
        .await
        .unwrap();

        let published = bus.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, UserRegistered::SUBJECT);
    }
}
