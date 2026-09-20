use axum::http::{HeaderValue, Uri};

#[derive(Clone)]
pub struct Config {
    pub(crate) allowed_origin: HeaderValue,
    pub(crate) cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let origin =
            std::env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_owned());
        // Default to HTTPS; local HTTP development must explicitly disable this flag.
        let secure = std::env::var("COOKIE_SECURE")
            .unwrap_or_else(|_| "true".to_owned())
            .parse::<bool>()?;
        Self::new(&origin, secure)
    }

    pub fn new(origin: &str, cookie_secure: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let uri: Uri = origin.parse()?;
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
            return Err(
                "FRONTEND_ORIGIN must be an HTTP(S) origin without a path or trailing slash".into(),
            );
        }
        Ok(Self {
            allowed_origin: origin.parse()?,
            cookie_secure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_origins_are_accepted() {
        assert!(Config::new("http://localhost:3000", false).is_ok());
        for origin in [
            "*",
            "null",
            "https://example.com/path",
            "https://example.com/",
            "https://user@example.com",
            "https://example.com?x=1",
        ] {
            assert!(Config::new(origin, true).is_err(), "{origin}");
        }
    }
}
