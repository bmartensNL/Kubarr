use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins. Empty means any origin is allowed (dev convenience).
    /// Set `KUBARR_ALLOWED_ORIGINS` to a comma-separated list to restrict origins.
    pub allowed_origins: Vec<String>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let allowed_origins = env::var("KUBARR_ALLOWED_ORIGINS")
            .map(|s| {
                s.split(',')
                    .map(|o| o.trim().to_string())
                    .filter(|o| !o.is_empty())
                    .collect()
            })
            .unwrap_or_default();

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
