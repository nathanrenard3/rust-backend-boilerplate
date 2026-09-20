mod dto;
mod endpoints;
mod error;
mod service;
pub mod session;

use crate::config::Config;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use axum_login::AuthManagerLayerBuilder;
use error::AuthError;
use sea_orm::DatabaseConnection;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite};

pub async fn router(
    db: DatabaseConnection,
    config: Config,
) -> Result<Router<DatabaseConnection>, AuthError> {
    let auth_service = service::AuthService::new(db.clone()).await?;
    let session_layer = SessionManagerLayer::new(session::SeaOrmSessionStore(db))
        .with_name("session_id")
        .with_path("/")
        .with_http_only(true)
        .with_secure(config.cookie_secure)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::days(1)));
    let auth_layer = AuthManagerLayerBuilder::new(auth_service, session_layer).build();
    // Use the peer IP, not an X-Forwarded-For header that the client could forge.
    let governor = GovernorConfigBuilder::default()
        .per_second(10)
        .burst_size(5)
        .finish()
        .expect("valid rate limit");
    let limiter = governor.limiter().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.retain_recent();
        }
    });
    Ok(Router::new()
        .route("/register", post(endpoints::register))
        .route("/login", post(endpoints::login))
        // Apply this limit only to the routes declared above: registration and login.
        .route_layer(GovernorLayer::new(governor))
        .route("/me", get(endpoints::me))
        .route("/logout", post(endpoints::logout))
        .layer(auth_layer)
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(config, protect_requests))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        )))
}

async fn protect_requests(
    State(config): State<Config>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    if !request.method().is_safe()
        && (request.headers().get("x-csrf-protection") != Some(&HeaderValue::from_static("1"))
            || request
                .headers()
                .get(header::ORIGIN)
                .is_some_and(|origin| origin != config.allowed_origin))
    {
        // Forms cannot set this header; CORS restricts cross-origin JavaScript requests.
        return Err(AuthError::Forbidden);
    }
    Ok(next.run(request).await)
}
