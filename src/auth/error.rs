use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("{0}")]
    InvalidInput(&'static str),
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

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::EmailTaken => StatusCode::CONFLICT,
            Self::InvalidCredentials | Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Internal | Self::Database(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = if status.is_server_error() {
            // Internal errors may contain SQL details; keep both the response and log generic.
            tracing::error!("Authentication operation failed");
            "Internal server error".to_owned()
        } else {
            self.to_string()
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}
