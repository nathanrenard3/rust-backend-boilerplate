mod auth;
pub mod config;
mod error;
mod extractors;
mod health;
mod middleware;
mod routes;
mod users;

use axum::Router;
use migration::MigratorTrait;
use sea_orm::DatabaseConnection;

pub async fn app(
    db: DatabaseConnection,
    config: config::Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    migration::Migrator::up(&db, None).await?;
    routes::router(db, config).await
}

pub async fn delete_expired_sessions(
    db: DatabaseConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    use tower_sessions::session_store::ExpiredDeletion;
    auth::session::SeaOrmSessionStore(db)
        .delete_expired()
        .await?;
    Ok(())
}
