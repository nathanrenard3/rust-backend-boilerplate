use crate::error::ApiError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("{message}")]
    InvalidInput {
        field: &'static str,
        message: &'static str,
    },
    #[error("Email already registered")]
    EmailTaken,
    #[error("Invalid email or password")]
    InvalidCredentials,
    #[error("Authentication required")]
    Unauthorized,
    #[error("Request origin or CSRF header rejected")]
    Forbidden,
    #[error("Internal server error")]
    Internal,
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidInput { field, message } => Self::validation(field, message),
            AuthError::EmailTaken => Self::new(
                StatusCode::CONFLICT,
                "email_taken",
                "Email already registered",
            ),
            AuthError::InvalidCredentials => Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "Invalid email or password",
            ),
            AuthError::Unauthorized => Self::from_status(StatusCode::UNAUTHORIZED),
            AuthError::Forbidden => Self::from_status(StatusCode::FORBIDDEN),
            error => {
                let category = match error {
                    AuthError::Database(_) => "database",
                    AuthError::Task(_) => "task",
                    _ => "internal",
                };
                tracing::error!(
                    operation = "authentication",
                    category,
                    "Authentication operation failed"
                );
                Self::from_status(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        ApiError::from(self).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::Request,
        routing::get,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn database_failure_does_not_expose_internal_details() {
        let app = Router::new().route(
            "/",
            get(|| async {
                AuthError::Database(sea_orm::DbErr::Custom("private SQL and credentials".into()))
            }),
        );
        let response = app.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"code":"internal_error", "message":"Internal server error"})
        );
    }
}
