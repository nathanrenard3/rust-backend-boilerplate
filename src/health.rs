use axum::{extract::State, http::StatusCode};
use sea_orm::DatabaseConnection;

pub async fn index() -> &'static str {
    "Rust API is running"
}

pub async fn health(State(db): State<DatabaseConnection>) -> StatusCode {
    match db.ping().await {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            tracing::error!(%error, "PostgreSQL unavailable");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
