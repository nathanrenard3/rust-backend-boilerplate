use axum::{
    Router,
    http::{HeaderName, Method, header},
    routing::get,
};
use sea_orm::DatabaseConnection;
use tower_http::cors::CorsLayer;

use crate::{auth, config::Config, health};

pub async fn router(
    db: DatabaseConnection,
    config: Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Allow credentialed browser requests only from the configured frontend origin.
    let cors = CorsLayer::new()
        .allow_origin(config.allowed_origin.clone())
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-csrf-protection"),
        ]);
    Ok(Router::new()
        .route("/", get(health::index))
        .route("/health", get(health::health))
        .nest("/auth", auth::router(db.clone(), config).await?)
        .layer(cors)
        .with_state(db))
}
