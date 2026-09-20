use super::{
    CurrentUser,
    dto::{Credentials, UserResponse},
    error::AuthError,
    service::AuthService,
};
use crate::extractors::ApiJson;
use axum::{Json, http::StatusCode};
use tower_sessions::Expiry;

type AuthSession = axum_login::AuthSession<AuthService>;

pub async fn register(
    auth: AuthSession,
    ApiJson(credentials): ApiJson<Credentials>,
) -> Result<(StatusCode, Json<UserResponse>), AuthError> {
    let user = auth.backend.register(credentials).await?;
    Ok((StatusCode::CREATED, Json(user.into())))
}

pub async fn login(
    mut auth: AuthSession,
    ApiJson(credentials): ApiJson<Credentials>,
) -> Result<Json<UserResponse>, AuthError> {
    let user = auth
        .authenticate(credentials.normalize()?)
        .await
        .map_err(|_| AuthError::Internal)?
        .ok_or(AuthError::InvalidCredentials)?;
    // logout() alone does not always renew the session ID in this library version.
    // Explicit rotation invalidates the previous cookie after a successful login.
    auth.logout().await.map_err(|_| AuthError::Internal)?;
    auth.session
        .cycle_id()
        .await
        .map_err(|_| AuthError::Internal)?;
    auth.session.set_expiry(Some(Expiry::AtDateTime(
        time::OffsetDateTime::now_utc() + time::Duration::days(1),
    )));
    auth.login(&user).await.map_err(|_| AuthError::Internal)?;
    Ok(Json(user.into()))
}

pub async fn me(CurrentUser(user): CurrentUser) -> Json<UserResponse> {
    Json(user.into())
}

pub async fn logout(mut auth: AuthSession) -> Result<StatusCode, AuthError> {
    auth.logout().await.map_err(|_| AuthError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}
