use super::*;
use axum::body::{Body, to_bytes};

async fn assert_error(response: axum::response::Response, code: &str) -> Value {
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = json_body(response).await;
    assert_eq!(body["code"], code);
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    assert!(body.get("error").is_none());
    body
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn validation_identifies_the_field() {
    let app = TestApp::new().await;
    for (field, body) in [
        ("email", json!({"email":"invalid", "password":PASSWORD})),
        ("password", json!({"email":app.email, "password":"short"})),
    ] {
        let response = app
            .send(
                request("POST", "/auth/register", body, None),
                StatusCode::BAD_REQUEST,
            )
            .await;
        let error = assert_error(response, "validation_error").await;
        assert!(
            error["errors"][field]
                .as_array()
                .is_some_and(|messages| !messages.is_empty())
        );
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn authentication_errors_have_distinct_codes() {
    let app = TestApp::new().await;
    let response = app.send(me(None), StatusCode::UNAUTHORIZED).await;
    let body = assert_error(response, "authentication_required").await;
    assert!(body.get("errors").is_none());
    let response = app
        .send(
            request("POST", "/auth/login", app.credentials(), None),
            StatusCode::UNAUTHORIZED,
        )
        .await;
    assert_error(response, "invalid_credentials").await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn conflict_and_csrf_use_the_common_format() {
    let app = TestApp::new().await;
    app.register().await;
    let response = app
        .send(
            request("POST", "/auth/register", app.credentials(), None),
            StatusCode::CONFLICT,
        )
        .await;
    assert_error(response, "email_taken").await;
    let mut req = request("POST", "/auth/login", app.credentials(), None);
    req.headers_mut().remove("x-csrf-protection");
    req.headers_mut()
        .insert("origin", "http://localhost:3000".parse().unwrap());
    let response = app.send(req, StatusCode::FORBIDDEN).await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://localhost:3000"
    );
    assert_error(response, "forbidden").await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn json_failures_preserve_http_statuses() {
    let app = TestApp::new().await;
    for (payload, status, code) in [
        ("{".to_owned(), StatusCode::BAD_REQUEST, "invalid_json"),
        (
            "{}".to_owned(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_payload",
        ),
        (
            json!({"email":42,"password":PASSWORD}).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_payload",
        ),
        (
            "x".repeat(17 * 1024),
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        ),
    ] {
        let mut req = request("POST", "/auth/register", Value::Null, None);
        *req.body_mut() = Body::from(payload);
        assert_error(app.send(req, status).await, code).await;
    }
    let mut req = request("POST", "/auth/register", app.credentials(), None);
    req.headers_mut().remove("content-type");
    assert_error(
        app.send(req, StatusCode::UNSUPPORTED_MEDIA_TYPE).await,
        "unsupported_media_type",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn router_errors_and_head_obey_http_contract() {
    let app = TestApp::new().await;
    assert_error(
        app.send(
            request("GET", "/missing", Value::Null, None),
            StatusCode::NOT_FOUND,
        )
        .await,
        "not_found",
    )
    .await;
    let response = app
        .send(
            request("GET", "/auth/login", Value::Null, None),
            StatusCode::METHOD_NOT_ALLOWED,
        )
        .await;
    assert!(
        response.headers()["allow"]
            .to_str()
            .unwrap()
            .contains("POST")
    );
    assert_error(response, "method_not_allowed").await;
    for (path, status) in [
        ("/missing", StatusCode::NOT_FOUND),
        ("/auth/me", StatusCode::UNAUTHORIZED),
    ] {
        let response = app
            .send(request("HEAD", path, Value::Null, None), status)
            .await;
        assert!(
            to_bytes(response.into_body(), 4096)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn database_failure_is_safe_and_health_is_unavailable() {
    let app = TestApp::new().await;
    app.db.clone().close().await.unwrap();
    assert_error(
        app.send(
            request("GET", "/health", Value::Null, None),
            StatusCode::SERVICE_UNAVAILABLE,
        )
        .await,
        "service_unavailable",
    )
    .await;
    let body = assert_error(
        app.send(
            request("POST", "/auth/login", app.credentials(), None),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await,
        "internal_error",
    )
    .await;
    assert_eq!(body["message"], "Internal server error");
    assert_eq!(body.as_object().unwrap().len(), 2);
}
