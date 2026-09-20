mod auth;
pub mod config;
mod health;
mod routes;
mod users;

use axum::Router;
use sea_orm::DatabaseConnection;

pub async fn app(
    db: DatabaseConnection,
    config: config::Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Synchronize the schema from entities; this does not create a versioned migration history.
    db.get_schema_builder()
        .register(users::user::Entity)
        .register(auth::session::model::Entity)
        .sync(&db)
        .await?;
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
