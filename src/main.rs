mod auth;
mod routes;
mod settings;

use sea_orm::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")?;
    let db = Database::connect(database_url).await?;

    let app = routes::router(db);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    tracing::info!("API listening on 0.0.0.0:8000");
    axum::serve(listener, app).await?;

    Ok(())
}
