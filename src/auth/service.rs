use super::{dto::Credentials, error::AuthError};
use crate::users::user;
use axum_login::{AuthUser, AuthnBackend, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, SqlErr,
};
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthService {
    db: DatabaseConnection,
    dummy_hash: Arc<String>,
    // Argon2 is memory-intensive; share this concurrency limit across hashing and verification.
    password_slots: Arc<tokio::sync::Semaphore>,
}

impl AuthService {
    pub async fn new(db: DatabaseConnection) -> Result<Self, AuthError> {
        let dummy_hash =
            tokio::task::spawn_blocking(|| password_auth::generate_hash(Uuid::new_v4().as_bytes()))
                .await?;
        Ok(Self {
            db,
            dummy_hash: Arc::new(dummy_hash),
            password_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        })
    }

    pub async fn register(&self, credentials: Credentials) -> Result<user::Model, AuthError> {
        let credentials = credentials.normalize()?;
        if credentials.password.chars().count() < 15 {
            return Err(AuthError::InvalidInput {
                field: "password",
                message: "Password must contain at least 15 characters",
            });
        }
        let permit = self
            .password_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AuthError::Internal)?;
        // Run CPU-bound work off the async executor and hold the permit until it finishes.
        let hash = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            password_auth::generate_hash(credentials.password)
        })
        .await?;
        let user = user::ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(credentials.email),
            password_hash: Set(hash),
            created_at: Set(OffsetDateTime::now_utc().unix_timestamp()),
        }
        .insert(&self.db)
        .await;
        // The database constraint also handles concurrent registrations with the same email.
        match user {
            Ok(user) => Ok(user),
            Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                Err(AuthError::EmailTaken)
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl AuthUser for user::Model {
    type Id = Uuid;
    fn id(&self) -> Self::Id {
        self.id
    }
    fn session_auth_hash(&self) -> &[u8] {
        // Changing the password invalidates sessions tied to the previous hash.
        self.password_hash.as_bytes()
    }
}

impl AuthnBackend for AuthService {
    type User = user::Model;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        credentials: Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = user::Entity::find()
            .filter(user::Column::Email.eq(credentials.email))
            .one(&self.db)
            .await?;
        // Avoid a noticeably faster response that would reveal a missing account.
        let hash = user
            .as_ref()
            .map(|user| user.password_hash.clone())
            .unwrap_or_else(|| (*self.dummy_hash).clone());
        let permit = self
            .password_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AuthError::Internal)?;
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            password_auth::verify_password(credentials.password, &hash).is_ok()
        })
        .await?;
        Ok(user.filter(|_| valid))
    }

    async fn get_user(&self, id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(user::Entity::find_by_id(*id).one(&self.db).await?)
    }
}
