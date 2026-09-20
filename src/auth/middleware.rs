use axum::{
    extract::{Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

use super::error::AuthError;
use crate::config::Config;

pub(super) async fn protect_requests(
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
