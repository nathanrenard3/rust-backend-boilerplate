use axum::{
    body::Body,
    extract::Request,
    http::{Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::error::ApiError;

pub(crate) async fn normalize_errors(request: Request, next: Next) -> Response {
    let is_head = request.method() == Method::HEAD;
    let response = next.run(request).await;
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    let mut response = if ApiError::is_formatted(&response) {
        response
    } else {
        let (mut parts, _) = response.into_parts();
        // These headers describe the replaced body; retain policy and protocol headers.
        for name in [
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
            header::CONTENT_ENCODING,
            header::CONTENT_RANGE,
            header::ETAG,
            header::LAST_MODIFIED,
            header::TRANSFER_ENCODING,
            header::TRAILER,
        ] {
            parts.headers.remove(name);
        }
        for name in ["content-md5", "digest", "content-digest", "repr-digest"] {
            parts.headers.remove(name);
        }
        let (formatted, body) = ApiError::from_status(status).into_response().into_parts();
        parts.headers.extend(formatted.headers);
        parts.extensions.extend(formatted.extensions);
        Response::from_parts(parts, body)
    };
    if is_head {
        *response.body_mut() = Body::empty();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::to_bytes,
        http::{HeaderValue, StatusCode},
        middleware::from_fn,
        routing::get,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn replacement_keeps_protocol_headers_and_removes_representation_headers() {
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    let mut response = (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "private database failure",
                    )
                        .into_response();
                    for (name, value) in [
                        ("cache-control", "no-store"),
                        ("access-control-allow-origin", "https://example.com"),
                        ("allow", "GET"),
                        ("retry-after", "10"),
                        ("content-encoding", "gzip"),
                        ("etag", "old"),
                        ("content-digest", "old"),
                    ] {
                        response.headers_mut().insert(
                            axum::http::HeaderName::from_static(name),
                            HeaderValue::from_static(value),
                        );
                    }
                    response
                        .headers_mut()
                        .append(header::SET_COOKIE, HeaderValue::from_static("a=1"));
                    response
                        .headers_mut()
                        .append(header::SET_COOKIE, HeaderValue::from_static("b=2"));
                    response
                }),
            )
            .layer(from_fn(normalize_errors));
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        for (name, value) in [
            ("cache-control", "no-store"),
            ("access-control-allow-origin", "https://example.com"),
            ("allow", "GET"),
            ("retry-after", "10"),
        ] {
            assert_eq!(response.headers()[name], value);
        }
        assert_eq!(
            response
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .count(),
            2
        );
        for name in ["content-encoding", "etag", "content-digest"] {
            assert!(!response.headers().contains_key(name));
        }
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let length: usize = response.headers()[header::CONTENT_LENGTH]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(length, body.len());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"code":"internal_error", "message":"Internal server error"})
        );
    }

    #[tokio::test]
    async fn canonical_errors_and_successes_are_preserved() {
        let app = Router::new()
            .route(
                "/error",
                get(|| async { ApiError::validation("email", "Invalid email address") }),
            )
            .route("/success", get(|| async { (StatusCode::OK, "unchanged") }))
            .route(
                "/redirect",
                get(|| async { axum::response::Redirect::temporary("/success") }),
            )
            .layer(from_fn(normalize_errors));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/error")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], "validation_error");
        assert_eq!(value["errors"]["email"][0], "Invalid email address");
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/error")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );
        let redirect = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/redirect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(redirect.headers()[header::LOCATION], "/success");
        assert!(
            to_bytes(redirect.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/success")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            "unchanged"
        );
    }
}
