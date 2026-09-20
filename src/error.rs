use std::collections::BTreeMap;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<BTreeMap<&'static str, Vec<&'static str>>>,
}

#[derive(Clone)]
struct FormattedError;

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            errors: None,
        }
    }

    pub(crate) fn validation(field: &'static str, message: &'static str) -> Self {
        Self {
            errors: Some(BTreeMap::from([(field, vec![message])])),
            ..Self::new(StatusCode::BAD_REQUEST, "validation_error", "Invalid input")
        }
    }

    pub(crate) fn from_status(status: StatusCode) -> Self {
        let (code, message) = match status {
            StatusCode::BAD_REQUEST => ("bad_request", "Invalid request"),
            StatusCode::UNAUTHORIZED => ("authentication_required", "Authentication required"),
            StatusCode::FORBIDDEN => ("forbidden", "Request origin or CSRF header rejected"),
            StatusCode::NOT_FOUND => ("not_found", "Resource not found"),
            StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "Method not allowed"),
            StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "Request body too large"),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => {
                ("unsupported_media_type", "Expected JSON content type")
            }
            StatusCode::UNPROCESSABLE_ENTITY => ("invalid_payload", "Invalid request payload"),
            StatusCode::TOO_MANY_REQUESTS => ("rate_limited", "Too many requests"),
            StatusCode::SERVICE_UNAVAILABLE => ("service_unavailable", "Service unavailable"),
            _ if status.is_server_error() => ("internal_error", "Internal server error"),
            _ => ("request_error", "Request failed"),
        };
        Self::new(status, code, message)
    }

    pub(crate) fn is_formatted(response: &Response) -> bool {
        response.extensions().get::<FormattedError>().is_some()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self)).into_response();
        response.extensions_mut().insert(FormattedError);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde_json::json;

    #[tokio::test]
    async fn validation_has_field_errors() {
        let response = ApiError::validation("email", "Invalid email address").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(ApiError::is_formatted(&response));
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!({
                "code": "validation_error", "message": "Invalid input",
                "errors": {"email": ["Invalid email address"]}
            })
        );
    }

    #[tokio::test]
    async fn other_errors_omit_field_errors() {
        let response = ApiError::from_status(StatusCode::INTERNAL_SERVER_ERROR).into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!({
                "code": "internal_error", "message": "Internal server error"
            })
        );
    }
}
