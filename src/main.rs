use rust_backend_boilerplate::{app, config::Config, delete_expired_sessions};
use sea_orm::{ConnectOptions, Database};

const SESSION_CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;
    let mut options = ConnectOptions::new(config.database.url);
    // Keep queries and sensitive parameters out of SQL logs.
    // SeaORM applies this timeout to acquiring a connection from the pool.
    options
        .max_connections(config.database.max_connections)
        .connect_timeout(config.database.connect_timeout)
        .sqlx_logging(false);
    let db = Database::connect(options).await?;

    // Cloning this handle shares the pool; it does not open a new connection.
    let app = app(db.clone(), config.auth).await?;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SESSION_CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            if delete_expired_sessions(db.clone()).await.is_err() {
                tracing::error!("Could not remove expired sessions");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(config.server.address).await?;
    tracing::info!(address = %listener.local_addr()?, "API listening");
    // The peer address is required for per-IP rate limiting.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
