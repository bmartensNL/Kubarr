use std::env;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub oauth2_enabled: bool,
    pub oauth2_issuer_url: String,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            oauth2_enabled: env::var("KUBARR_OAUTH2_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            oauth2_issuer_url: env::var("KUBARR_OAUTH2_ISSUER_URL")
                .unwrap_or_else(|_| "http://kubarr:8000/auth".to_string()),
        }
    }
}
