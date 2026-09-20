use super::*;
use crate::auth::CurrentUser;
use axum::{
    Json,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    response::Response,
};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn protected(CurrentUser(user): CurrentUser) -> Json<Value> {
    Json(json!({"id": user.id, "email": user.email}))
}

async fn test_app() -> Router {
    let mut options =
        ConnectOptions::new(std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL"));
    options.max_connections(2).sqlx_logging(false);
    let admin = Database::connect(options.clone()).await.unwrap();
    let schema = format!("shared_auth_{}", uuid::Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    admin.close().await.unwrap();
    options.set_schema_search_path(schema);
    let db = Database::connect(options).await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    router_with_routes(
        db,
        Config::new("http://localhost:3000", false).unwrap(),
        Router::new().route("/test-profile", get(protected).post(protected)),
    )
    .await
    .unwrap()
}

fn request(method: &str, path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-csrf-protection", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let mut request = builder
        .body(Body::from(
            json!({"email":"shared@example.com", "password":"a long test password!"}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
    ));
    request
}

async fn send(app: &Router, request: Request<Body>, status: StatusCode) -> Response {
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), status);
    response
}

async fn login(app: &Router) -> String {
    send(
        app,
        request("POST", "/auth/register", None),
        StatusCode::CREATED,
    )
    .await;
    let response = send(app, request("POST", "/auth/login", None), StatusCode::OK).await;
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn feature_requires_authentication() {
    let app = test_app().await;
    send(
        &app,
        request("GET", "/test-profile", None),
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn feature_uses_login_session() {
    let app = test_app().await;
    let cookie = login(&app).await;
    let response = send(
        &app,
        request("GET", "/test-profile", Some(&cookie)),
        StatusCode::OK,
    )
    .await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["email"], "shared@example.com");
    let me = send(
        &app,
        request("GET", "/auth/me", Some(&cookie)),
        StatusCode::OK,
    )
    .await;
    let me: Value = serde_json::from_slice(&to_bytes(me.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body, me);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn logout_revokes_feature_access() {
    let app = test_app().await;
    let cookie = login(&app).await;
    send(
        &app,
        request("POST", "/auth/logout", Some(&cookie)),
        StatusCode::NO_CONTENT,
    )
    .await;
    send(
        &app,
        request("GET", "/test-profile", Some(&cookie)),
        StatusCode::UNAUTHORIZED,
    )
    .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn feature_mutations_require_csrf_protection() {
    let app = test_app().await;
    let cookie = login(&app).await;
    let mut missing = request("POST", "/test-profile", Some(&cookie));
    missing.headers_mut().remove("x-csrf-protection");
    send(&app, missing, StatusCode::FORBIDDEN).await;
    let mut untrusted = request("POST", "/test-profile", Some(&cookie));
    untrusted
        .headers_mut()
        .insert("origin", "https://untrusted.example".parse().unwrap());
    send(&app, untrusted, StatusCode::FORBIDDEN).await;
    let mut trusted = request("POST", "/test-profile", Some(&cookie));
    trusted
        .headers_mut()
        .insert("origin", "http://localhost:3000".parse().unwrap());
    send(&app, trusted, StatusCode::OK).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn public_routes_do_not_require_sessions() {
    let app = test_app().await;
    for path in ["/", "/health"] {
        let response = send(
            &app,
            request("GET", path, Some("session_id=invalid")),
            StatusCode::OK,
        )
        .await;
        assert!(!response.headers().contains_key("set-cookie"));
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn cors_preflight_allows_crud_without_session_or_csrf() {
    let app = test_app().await;
    for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/test-profile")
            .header("origin", "http://localhost:3000")
            .header("access-control-request-method", method)
            .header(
                "access-control-request-headers",
                "content-type,x-csrf-protection",
            )
            .body(Body::empty())
            .unwrap();
        let response = send(&app, request, StatusCode::OK).await;
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "http://localhost:3000"
        );
        assert_eq!(
            response.headers()["access-control-allow-credentials"],
            "true"
        );
        assert!(
            response.headers()["access-control-allow-methods"]
                .to_str()
                .unwrap()
                .split(',')
                .any(|allowed| allowed.trim() == method)
        );
    }
}
