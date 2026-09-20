use super::{error::AuthError, service::AuthService};
use crate::users::user;
use axum::{extract::FromRequestParts, http::request::Parts};

pub(crate) struct CurrentUser(pub(crate) user::Model);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = axum_login::AuthSession::<AuthService>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Internal)?;
        auth.user.map(Self).ok_or(AuthError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::Request, http::StatusCode, response::IntoResponse};

    #[tokio::test]
    async fn missing_auth_layer_is_a_server_error() {
        let (mut parts, _) = Request::new(()).into_parts();
        let Err(error) = CurrentUser::from_request_parts(&mut parts, &()).await else {
            panic!("Extraction must fail without the authentication layer");
        };

        assert!(matches!(error, AuthError::Internal));
        assert_eq!(
            error.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
