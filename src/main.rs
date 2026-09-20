mod shutdown;

use rust_backend_boilerplate::{app, config::Config, delete_expired_sessions};
use sea_orm::{ConnectOptions, Database};
use std::time::Duration;

const SESSION_CLEANUP_INTERVAL: Duration = Duration::from_secs(3600);
const REQUEST_DRAIN_TIMEOUT: Duration = Duration::from_secs(20);
const DATABASE_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run());
    // Password hashing runs on blocking workers and cannot be cancelled once started.
    runtime.shutdown_timeout(Duration::from_secs(1));
    result
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(config.log_filter)
        .init();
    let mut signals = shutdown::Signals::new()?;
    let mut options = ConnectOptions::new(config.database.url);
    // SeaORM applies this timeout to acquiring a connection from the pool.
    options
        .max_connections(config.database.max_connections)
        .connect_timeout(config.database.connect_timeout)
        .sqlx_logging(false);
    let db = Database::connect(options).await?;
    let app = app(db.clone(), config.auth).await?;
    let listener = tokio::net::TcpListener::bind(config.server.address).await?;
    tracing::info!(address = %listener.local_addr()?, "API listening");

    let cleanup_db = db.clone();
    let cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(SESSION_CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            if delete_expired_sessions(cleanup_db.clone()).await.is_err() {
                tracing::error!("Could not remove expired sessions");
            }
        }
    });
    let result = shutdown::serve(listener, app, signals.wait(), REQUEST_DRAIN_TIMEOUT).await;
    cleanup.abort();
    let _ = cleanup.await;
    match tokio::time::timeout(DATABASE_CLOSE_TIMEOUT, db.close()).await {
        Ok(Ok(())) => tracing::info!("Database pool closed"),
        Ok(Err(_)) => tracing::error!("Could not close database pool"),
        Err(_) => tracing::warn!("Database pool close timed out"),
    }
    result?;
    Ok(())
}
