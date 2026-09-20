use std::{env::VarError, net::SocketAddr, time::Duration};

use axum::http::{HeaderValue, Uri};
use sea_orm::sqlx::postgres::PgConnectOptions;

const DEFAULT_SESSION_TTL_SECONDS: u64 = 86_400;
const MAX_SESSION_TTL_SECONDS: u64 = 365 * 86_400;
const MAX_CONNECT_TIMEOUT_SECONDS: u64 = 300;
const MAX_CONNECTIONS: u64 = 1000;

pub struct Config {
    pub log_filter: tracing_subscriber::EnvFilter,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

pub struct ServerConfig {
    pub address: SocketAddr,
}

// Do not derive Debug: the URL can contain database credentials.
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connect_timeout: Duration,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub(crate) allowed_origin: HeaderValue,
    pub(crate) cookie_secure: bool,
    session_ttl: time::Duration,
}

#[derive(Debug, thiserror::Error)]
#[error("{variable} must be {expected}")]
pub struct ConfigError {
    variable: &'static str,
    expected: &'static str,
}

fn invalid(variable: &'static str, expected: &'static str) -> ConfigError {
    ConfigError { variable, expected }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name))
    }

    fn from_lookup(lookup: impl Fn(&str) -> Result<String, VarError>) -> Result<Self, ConfigError> {
        let read = |name: &'static str, default: Option<&str>| match lookup(name) {
            Ok(value) => Ok(value),
            Err(VarError::NotPresent) => default
                .map(str::to_owned)
                .ok_or_else(|| invalid(name, "set to a valid PostgreSQL URL")),
            Err(VarError::NotUnicode(_)) => Err(invalid(name, "valid Unicode")),
        };
        let log_filter = tracing_subscriber::EnvFilter::builder()
            .with_regex(false)
            .parse(read("RUST_LOG", Some("info"))?)
            .map_err(|_| invalid("RUST_LOG", "a valid tracing filter"))?;
        let address = read("SERVER_ADDR", Some("0.0.0.0:8000"))?
            .parse::<SocketAddr>()
            .ok()
            .filter(|address| address.port() != 0)
            .ok_or_else(|| invalid("SERVER_ADDR", "an IP address and nonzero port"))?;
        let url = read("DATABASE_URL", None)?;
        if !matches!(url.split_once("://"), Some(("postgres" | "postgresql", _)))
            || url.parse::<PgConnectOptions>().is_err()
        {
            return Err(invalid("DATABASE_URL", "a valid PostgreSQL URL"));
        }
        let max_connections = positive_number(
            "DATABASE_MAX_CONNECTIONS",
            &read("DATABASE_MAX_CONNECTIONS", Some("10"))?,
            MAX_CONNECTIONS,
            "an integer between 1 and 1000",
        )? as u32;
        let timeout = positive_number(
            "DATABASE_CONNECT_TIMEOUT_SECONDS",
            &read("DATABASE_CONNECT_TIMEOUT_SECONDS", Some("10"))?,
            MAX_CONNECT_TIMEOUT_SECONDS,
            "an integer between 1 and 300",
        )?;
        let secure = read("COOKIE_SECURE", Some("true"))?
            .parse::<bool>()
            .map_err(|_| invalid("COOKIE_SECURE", "true or false"))?;
        let ttl = positive_number(
            "SESSION_TTL_SECONDS",
            &read("SESSION_TTL_SECONDS", Some("86400"))?,
            MAX_SESSION_TTL_SECONDS,
            "an integer between 1 and 31536000",
        )?;
        let auth = AuthConfig::new(
            &read("FRONTEND_ORIGIN", Some("http://localhost:3000"))?,
            secure,
        )?
        .with_session_ttl_seconds(ttl)?;
        Ok(Self {
            log_filter,
            server: ServerConfig { address },
            database: DatabaseConfig {
                url,
                max_connections,
                connect_timeout: Duration::from_secs(timeout),
            },
            auth,
        })
    }
}

fn positive_number(
    name: &'static str,
    value: &str,
    maximum: u64,
    expected: &'static str,
) -> Result<u64, ConfigError> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| (1..=maximum).contains(value))
        .ok_or_else(|| invalid(name, expected))
}

impl AuthConfig {
    pub fn new(origin: &str, cookie_secure: bool) -> Result<Self, ConfigError> {
        let error = || {
            invalid(
                "FRONTEND_ORIGIN",
                "an HTTP(S) origin without a path or trailing slash",
            )
        };
        let uri: Uri = origin.parse().map_err(|_| error())?;
        // This origin is later matched exactly against the browser's Origin header.
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri.authority().is_none()
            || uri
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
            || uri.path() != "/"
            || uri.query().is_some()
            || origin.ends_with('/')
        {
            return Err(error());
        }
        Ok(Self {
            allowed_origin: origin.parse().map_err(|_| error())?,
            cookie_secure,
            session_ttl: time::Duration::seconds(DEFAULT_SESSION_TTL_SECONDS as i64),
        })
    }

    pub fn with_session_ttl_seconds(mut self, seconds: u64) -> Result<Self, ConfigError> {
        if !(1..=MAX_SESSION_TTL_SECONDS).contains(&seconds) {
            return Err(invalid(
                "SESSION_TTL_SECONDS",
                "an integer between 1 and 31536000",
            ));
        }
        self.session_ttl = time::Duration::seconds(seconds as i64);
        Ok(self)
    }

    pub fn session_ttl(&self) -> time::Duration {
        self.session_ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::from_lookup(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
                .ok_or(VarError::NotPresent)
        })
    }

    #[test]
    fn defaults_and_overrides() {
        let defaults = parse(&[("DATABASE_URL", "postgres://localhost/test")]).unwrap();
        assert_eq!(defaults.server.address.to_string(), "0.0.0.0:8000");
        assert_eq!(defaults.database.max_connections, 10);
        assert_eq!(defaults.database.connect_timeout, Duration::from_secs(10));
        assert!(defaults.auth.cookie_secure);
        assert_eq!(defaults.auth.allowed_origin, "http://localhost:3000");
        assert_eq!(defaults.auth.session_ttl(), time::Duration::days(1));
        let custom = parse(&[
            (
                "DATABASE_URL",
                "postgresql://user:password@localhost:5432/test",
            ),
            ("SERVER_ADDR", "[::1]:9000"),
            ("DATABASE_MAX_CONNECTIONS", "1000"),
            ("DATABASE_CONNECT_TIMEOUT_SECONDS", "300"),
            ("SESSION_TTL_SECONDS", "31536000"),
            ("COOKIE_SECURE", "false"),
            ("FRONTEND_ORIGIN", "https://example.com"),
        ])
        .unwrap();
        assert_eq!(custom.server.address.to_string(), "[::1]:9000");
        assert_eq!(custom.database.max_connections, 1000);
        assert_eq!(custom.database.connect_timeout, Duration::from_secs(300));
        assert_eq!(custom.auth.session_ttl(), time::Duration::days(365));
        assert!(!custom.auth.cookie_secure);
        assert_eq!(custom.auth.allowed_origin, "https://example.com");
    }

    #[test]
    fn invalid_values_are_rejected_without_echoing_them() {
        assert_eq!(parse(&[]).err().unwrap().variable, "DATABASE_URL");
        for (name, values) in [
            (
                "DATABASE_URL",
                vec![
                    "",
                    "mysql://localhost/test",
                    "postgres://user:secret@[bad/test",
                    "postgres://localhost:invalid/test",
                ],
            ),
            (
                "SERVER_ADDR",
                vec!["", "localhost:8000", "0.0.0.0:0", "0.0.0.0:65536"],
            ),
            (
                "DATABASE_MAX_CONNECTIONS",
                vec!["0", "1001", "-1", "no", ""],
            ),
            (
                "DATABASE_CONNECT_TIMEOUT_SECONDS",
                vec!["0", "301", "18446744073709551616"],
            ),
            (
                "SESSION_TTL_SECONDS",
                vec!["0", "31536001", "18446744073709551615"],
            ),
            ("COOKIE_SECURE", vec!["", "1", "TRUE"]),
            (
                "FRONTEND_ORIGIN",
                vec![
                    "*",
                    "null",
                    "https://example.com/path",
                    "https://example.com/",
                    "https://user@example.com",
                    "https://example.com?x=1",
                ],
            ),
        ] {
            for value in values {
                let error = parse(&[(name, value), ("DATABASE_URL", "postgres://localhost/test")])
                    .err()
                    .unwrap();
                assert_eq!(error.variable, name);
                assert!(error.to_string().starts_with(name));
                assert!(!error.to_string().contains("secret"));
            }
        }
    }

    #[test]
    fn non_unicode_never_uses_defaults() {
        for name in [
            "RUST_LOG",
            "DATABASE_URL",
            "SERVER_ADDR",
            "DATABASE_MAX_CONNECTIONS",
            "DATABASE_CONNECT_TIMEOUT_SECONDS",
            "SESSION_TTL_SECONDS",
            "COOKIE_SECURE",
            "FRONTEND_ORIGIN",
        ] {
            let error = Config::from_lookup(|key| {
                if key == name {
                    Err(VarError::NotUnicode("private-value".into()))
                } else if key == "DATABASE_URL" {
                    Ok("postgres://localhost/test".into())
                } else {
                    Err(VarError::NotPresent)
                }
            })
            .err()
            .unwrap();
            assert_eq!(error.variable, name);
            assert!(!error.to_string().contains("private-value"));
        }
    }

    #[test]
    fn log_filter_is_validated() {
        let valid = parse(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("RUST_LOG", "warn,rust_backend_boilerplate=debug"),
        ])
        .unwrap();
        assert!(
            valid
                .log_filter
                .to_string()
                .contains("rust_backend_boilerplate=debug")
        );
        let error = parse(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("RUST_LOG", "crate=invalid-level"),
        ])
        .err()
        .unwrap();
        assert_eq!(error.variable, "RUST_LOG");
    }

    #[test]
    fn lower_bound_and_custom_ttl_are_validated() {
        let config = parse(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("DATABASE_MAX_CONNECTIONS", "1"),
            ("DATABASE_CONNECT_TIMEOUT_SECONDS", "1"),
            ("SESSION_TTL_SECONDS", "1"),
        ])
        .unwrap();
        assert_eq!(config.database.max_connections, 1);
        assert_eq!(config.database.connect_timeout, Duration::from_secs(1));
        assert_eq!(config.auth.session_ttl(), time::Duration::seconds(1));
        for ttl in [0, MAX_SESSION_TTL_SECONDS + 1, u64::MAX] {
            assert!(
                AuthConfig::new("http://localhost:3000", false)
                    .unwrap()
                    .with_session_ttl_seconds(ttl)
                    .is_err()
            );
        }
    }
}
