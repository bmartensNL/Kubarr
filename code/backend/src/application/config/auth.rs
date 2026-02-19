use std::env;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub oauth2_enabled: bool,
    pub oauth2_issuer_url: String,
    /// Number of failed login attempts before the account is locked
    pub lockout_threshold: u32,
    /// Duration in minutes for which the account is locked after reaching the threshold
    pub lockout_duration_minutes: u32,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            oauth2_enabled: env::var("KUBARR_OAUTH2_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            oauth2_issuer_url: env::var("KUBARR_OAUTH2_ISSUER_URL")
                .unwrap_or_else(|_| "http://kubarr:8000/auth".to_string()),
            lockout_threshold: env::var("KUBARR_LOCKOUT_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            lockout_duration_minutes: env::var("KUBARR_LOCKOUT_DURATION_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
        }
    }
}
