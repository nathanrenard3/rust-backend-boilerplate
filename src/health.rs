use axum::{extract::State, http::StatusCode};
use sea_orm::DatabaseConnection;

pub async fn index() -> &'static str {
    "Rust API is running"
}

pub async fn health(State(db): State<DatabaseConnection>) -> StatusCode {
    match db.ping().await {
        Ok(()) => StatusCode::OK,
        Err(_) => {
            tracing::error!(
                operation = "health_check",
                category = "database",
                "PostgreSQL unavailable"
            );
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
