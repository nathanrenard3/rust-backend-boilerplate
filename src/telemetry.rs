use axum::{
    extract::{MatchedPath, Request},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::Instrument;

pub(crate) async fn log_request(request: Request, next: Next) -> Response {
    // Generate our own ID; client headers must not control log correlation.
    let request_id = uuid::Uuid::new_v4().to_string();
    // Route templates exclude query strings and concrete path parameter values.
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("<unmatched>", MatchedPath::as_str)
        .to_owned();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %request.method(),
        route,
    );
    async move {
        let started = Instant::now();
        let mut response = next.run(request).await;
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id).expect("UUID is a valid header value"),
        );
        let status = response.status().as_u16();
        let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
        if response.status().is_server_error() {
            tracing::error!(%request_id, %method, %route, status, duration_ms, "Request completed");
        } else if response.status().is_client_error() {
            tracing::warn!(%request_id, %method, %route, status, duration_ms, "Request completed");
        } else {
            tracing::info!(%request_id, %method, %route, status, duration_ms, "Request completed");
        }
        response
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::StatusCode, middleware::from_fn, routing::get};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;

    #[derive(Clone)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn logs_safe_routes_and_correlates_error_responses_at_warn_level() {
        let buffer = LogBuffer(Arc::new(Mutex::new(Vec::new())));
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter("warn")
            .with_ansi(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        let subscriber = tracing::Dispatch::new(subscriber);
        let app = Router::new()
            .route("/users/{id}", get(|| async { StatusCode::UNAUTHORIZED }))
            .route(
                "/failure",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .layer(from_fn(log_request));
        let mut ids = Vec::new();
        for path in [
            "/users/private-path?token=private-query",
            "/private-unmatched",
            "/failure",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("cookie", "private-cookie")
                        .header("authorization", "Bearer private-token")
                        .header("x-request-id", "private-client-id")
                        .body(Body::from("private-body"))
                        .unwrap(),
                )
                .with_subscriber(subscriber.clone())
                .await
                .unwrap();
            let id = response.headers()["x-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            assert!(uuid::Uuid::parse_str(&id).is_ok());
            ids.push(id);
        }
        assert_ne!(ids[0], ids[1]);
        let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        for id in ids {
            assert!(logs.contains(&id));
        }
        assert!(logs.contains("/users/{id}"));
        assert!(logs.contains("<unmatched>"));
        assert!(logs.contains("status=401"));
        assert!(logs.contains("status=404"));
        assert!(logs.contains("status=500"));
        assert!(logs.contains("duration_ms="));
        assert!(!logs.contains("private-"), "{logs}");
    }
}
