#[path = "auth/errors.rs"]
mod errors;
mod support;

use axum::{extract::ConnectInfo, http::StatusCode};
use rust_backend_boilerplate::{app, config::Config};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use support::{PASSWORD, TestApp, cookie, json_body, me, request};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn health_checks_postgres() {
    let app = TestApp::new().await;
    app.send(request("GET", "/health", Value::Null, None), StatusCode::OK)
        .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn registration_normalizes_email_and_returns_only_public_fields() {
    let app = TestApp::new().await;
    let body = json!({"email": "  TEST@EXAMPLE.COM  ", "password": PASSWORD});
    let mut req = request("POST", "/auth/register", body, None);
    req.headers_mut()
        .insert("origin", "http://localhost:3000".parse().unwrap());
    let user = json_body(app.send(req, StatusCode::CREATED).await).await;

    assert_eq!(user["email"], app.email);
    assert!(user.get("password").is_none());
    assert!(user.get("password_hash").is_none());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn registration_hashes_the_password() {
    let app = TestApp::new().await;
    app.register().await;
    let row = app
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT password_hash FROM users WHERE email = $1",
            [app.email.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let hash: String = row.try_get("", "password_hash").unwrap();

    assert!(hash.starts_with("$argon2id$"));
    assert!(password_auth::verify_password(PASSWORD, &hash).is_ok());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn registration_rejects_invalid_credentials() {
    let app = TestApp::new().await;
    for body in [
        json!({"email":"invalid", "password":PASSWORD}),
        json!({"email":app.email, "password":"short"}),
    ] {
        app.send(
            request("POST", "/auth/register", body, None),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn registration_rejects_duplicate_email() {
    let app = TestApp::new().await;
    app.register().await;
    app.send(
        request("POST", "/auth/register", app.credentials(), None),
        StatusCode::CONFLICT,
    )
    .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn login_does_not_reveal_whether_an_email_exists() {
    let app = TestApp::new().await;
    app.register().await;
    let wrong = json!({"email":app.email, "password":"wrong password"});
    let unknown = json!({"email":"unknown@example.com", "password":"wrong password"});
    let wrong = app
        .send(
            request("POST", "/auth/login", wrong, None),
            StatusCode::UNAUTHORIZED,
        )
        .await;
    let unknown = app
        .send(
            request("POST", "/auth/login", unknown, None),
            StatusCode::UNAUTHORIZED,
        )
        .await;

    assert_eq!(json_body(wrong).await, json_body(unknown).await);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn login_issues_a_cookie_and_authenticates_me() {
    let app = TestApp::new().await;
    let user = app.register().await;
    let response = app.login().await;
    let header = response.headers()["set-cookie"].to_str().unwrap();
    assert!(header.contains("HttpOnly"));
    assert!(header.contains("SameSite=Lax"));
    assert!(header.contains("Path=/"));
    assert_eq!(response.headers()["cache-control"], "no-store");

    let cookie = cookie(&response);
    assert_eq!(json_body(response).await, user);
    assert_eq!(
        json_body(app.send(me(Some(&cookie)), StatusCode::OK).await).await,
        user
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn https_configuration_sets_secure_cookie() {
    let app = TestApp::with_config(Config::new("https://example.com", true).unwrap()).await;
    app.register().await;
    let response = app.login().await;
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Secure")
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn me_requires_a_session() {
    let app = TestApp::new().await;
    app.send(me(None), StatusCode::UNAUTHORIZED).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn session_survives_application_restart() {
    let mut test = TestApp::new().await;
    let cookie = test.session().await;
    test.api = app(
        test.db.clone(),
        Config::new("http://localhost:3000", false).unwrap(),
    )
    .await
    .unwrap();
    test.send(me(Some(&cookie)), StatusCode::OK).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn logout_revokes_the_session() {
    let app = TestApp::new().await;
    let cookie = app.session().await;
    app.send(
        request("POST", "/auth/logout", Value::Null, Some(&cookie)),
        StatusCode::NO_CONTENT,
    )
    .await;
    app.send(me(Some(&cookie)), StatusCode::UNAUTHORIZED).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn login_rotates_an_existing_session() {
    let app = TestApp::new().await;
    let old_cookie = app.session().await;
    let response = app
        .send(
            request("POST", "/auth/login", app.credentials(), Some(&old_cookie)),
            StatusCode::OK,
        )
        .await;
    let new_cookie = cookie(&response);

    assert_ne!(old_cookie, new_cookie);
    app.send(me(Some(&old_cookie)), StatusCode::UNAUTHORIZED)
        .await;
    app.send(me(Some(&new_cookie)), StatusCode::OK).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn expired_session_is_rejected() {
    let app = TestApp::new().await;
    let cookie = app.session().await;
    let id = cookie.split_once('=').unwrap().1;
    app.execute(
        "UPDATE auth_sessions SET expires_at = 0 WHERE id = $1",
        [id.into()],
    )
    .await;
    app.send(me(Some(&cookie)), StatusCode::UNAUTHORIZED).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn password_change_invalidates_existing_sessions() {
    let app = TestApp::new().await;
    let cookie = app.session().await;
    let hash = password_auth::generate_hash("another long test password!");
    app.execute(
        "UPDATE users SET password_hash = $1 WHERE email = $2",
        [hash.into(), app.email.clone().into()],
    )
    .await;
    app.send(me(Some(&cookie)), StatusCode::UNAUTHORIZED).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cors_allows_the_configured_frontend() {
    let app = TestApp::new().await;
    let mut req = request("OPTIONS", "/auth/login", Value::Null, None);
    for (name, value) in [
        ("origin", "http://localhost:3000"),
        ("access-control-request-method", "POST"),
        (
            "access-control-request-headers",
            "content-type,x-csrf-protection",
        ),
    ] {
        req.headers_mut().insert(name, value.parse().unwrap());
    }
    let response = app.send(req, StatusCode::OK).await;
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://localhost:3000"
    );
    assert_eq!(
        response.headers()["access-control-allow-credentials"],
        "true"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn registration_requires_csrf_header_and_trusted_origin() {
    let app = TestApp::new().await;
    let mut missing_header = request("POST", "/auth/register", app.credentials(), None);
    missing_header.headers_mut().remove("x-csrf-protection");
    app.send(missing_header, StatusCode::FORBIDDEN).await;

    let mut bad_origin = request("POST", "/auth/register", app.credentials(), None);
    bad_origin
        .headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    app.send(bad_origin, StatusCode::FORBIDDEN).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn logout_requires_csrf_header() {
    let app = TestApp::new().await;
    let cookie = app.session().await;
    let mut req = request("POST", "/auth/logout", Value::Null, Some(&cookie));
    req.headers_mut().remove("x-csrf-protection");
    app.send(req, StatusCode::FORBIDDEN).await;
    app.send(me(Some(&cookie)), StatusCode::OK).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn login_limits_repeated_attempts_from_the_same_ip() {
    let app = TestApp::new().await;
    for attempt in 0..6 {
        let body = json!({"email":app.email, "password":"wrong"});
        let mut req = request("POST", "/auth/login", body, None);
        req.extensions_mut().insert(ConnectInfo(
            "192.0.2.10:12345".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let expected = if attempt < 5 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        let response = app.send(req, expected).await;
        if expected == StatusCode::TOO_MANY_REQUESTS {
            assert!(response.headers().contains_key("retry-after"));
            assert_eq!(json_body(response).await["code"], "rate_limited");
        }
    }
}
