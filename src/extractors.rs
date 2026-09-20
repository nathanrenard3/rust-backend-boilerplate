use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
};
use serde::de::DeserializeOwned;

use crate::error::ApiError;

pub(crate) struct ApiJson<T>(pub(crate) T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| {
                let status = rejection.status();
                match rejection {
                    JsonRejection::JsonSyntaxError(_) => {
                        ApiError::new(status, "invalid_json", "Invalid JSON syntax")
                    }
                    JsonRejection::JsonDataError(_) => {
                        ApiError::new(status, "invalid_payload", "Invalid request payload")
                    }
                    _ => ApiError::from_status(status),
                }
            })
    }
}
