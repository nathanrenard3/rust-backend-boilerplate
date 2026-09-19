use axum::{Router, routing::get};
use sea_orm::DatabaseConnection;

use crate::{auth, settings};

pub fn router(db: DatabaseConnection) -> Router {
    Router::new()
        .route("/", get(settings::index))
        .route("/health", get(settings::health))
        .route("/auth", get(auth::index))
        .with_state(db)
}
