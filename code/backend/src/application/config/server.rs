use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins, parsed from `KUBARR_ALLOWED_ORIGINS` (comma-separated).
    /// When empty, any origin is allowed (dev convenience).
    pub allowed_origins: Vec<String>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let allowed_origins = env::var("KUBARR_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            host: env::var("KUBARR_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("KUBARR_API_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8000),
            allowed_origins,
        }
    }
}
