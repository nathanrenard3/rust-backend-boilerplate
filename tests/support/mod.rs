use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    response::Response,
};
use rust_backend_boilerplate::{app, config::AuthConfig};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
};
use tower::ServiceExt;

pub const PASSWORD: &str = "a long test password!";
static NEXT_IP: AtomicU32 = AtomicU32::new(1);

pub struct TestApp {
    pub api: Router,
    pub db: DatabaseConnection,
    pub email: String,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(AuthConfig::new("http://localhost:3000", false).unwrap()).await
    }

    pub async fn with_config(config: AuthConfig) -> Self {
        let url = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
        let mut options = ConnectOptions::new(url);
        options.max_connections(2).sqlx_logging(false);
        let admin = Database::connect(options.clone()).await.unwrap();
        // Each test owns a schema so parallel runs cannot share users or sessions.
        let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
        admin
            .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        admin.close().await.unwrap();
        options.set_schema_search_path(schema);
        let db = Database::connect(options).await.unwrap();
        let api = app(db.clone(), config).await.unwrap();
        Self {
            api,
            db,
            email: "test@example.com".to_owned(),
        }
    }

    pub fn credentials(&self) -> Value {
        json!({"email": self.email, "password": PASSWORD})
    }

    pub async fn send(&self, request: Request<Body>, expected: StatusCode) -> Response {
        let response = self.api.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        response
    }

    pub async fn register(&self) -> Value {
        let response = self
            .send(
                request("POST", "/auth/register", self.credentials(), None),
                StatusCode::CREATED,
            )
            .await;
        json_body(response).await
    }

    pub async fn login(&self) -> Response {
        self.send(
            request("POST", "/auth/login", self.credentials(), None),
            StatusCode::OK,
        )
        .await
    }

    pub async fn session(&self) -> String {
        self.register().await;
        cookie(&self.login().await)
    }

    pub async fn execute(&self, sql: &str, values: impl IntoIterator<Item = sea_orm::Value>) {
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                values,
            ))
            .await
            .unwrap();
    }
}

pub fn request(method: &str, path: &str, body: Value, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-csrf-protection", "1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let ip = std::net::Ipv4Addr::from(NEXT_IP.fetch_add(1, Ordering::Relaxed));
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((ip, 12345))));
    req
}

pub fn me(cookie: Option<&str>) -> Request<Body> {
    request("GET", "/auth/me", Value::Null, cookie)
}

pub async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap()
}

pub fn cookie(response: &Response) -> String {
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
