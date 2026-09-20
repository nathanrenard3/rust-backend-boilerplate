#[cfg(test)]
mod tests;

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
    router_with_routes(db, config, Router::new()).await
}

pub(crate) async fn router_with_routes(
    db: DatabaseConnection,
    config: Config,
    features: Router<DatabaseConnection>,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Allow credentialed browser requests only from the configured frontend origin.
    let cors = CorsLayer::new()
        .allow_origin(config.allowed_origin.clone())
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("x-csrf-protection"),
        ]);
    Ok(Router::new()
        .route("/", get(health::index))
        .route("/health", get(health::health))
        .merge(auth::application_routes(db.clone(), config, features).await?)
        .layer(cors)
        .with_state(db))
}
