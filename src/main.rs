use rust_backend_boilerplate::{app, config::Config, delete_expired_sessions};
use sea_orm::{ConnectOptions, Database};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")?;
    let mut options = ConnectOptions::new(database_url);
    // Keep queries and sensitive parameters out of SQL logs.
    options.sqlx_logging(false);
    let db = Database::connect(options).await?;

    // Cloning this handle shares the pool; it does not open a new connection.
    let app = app(db.clone(), Config::from_env()?).await?;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if delete_expired_sessions(db.clone()).await.is_err() {
                tracing::error!("Could not remove expired sessions");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    tracing::info!("API listening on 0.0.0.0:8000");
    // The peer address is required for per-IP rate limiting.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
