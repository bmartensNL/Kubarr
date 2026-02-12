use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            host: env::var("KUBARR_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("KUBARR_API_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8000),
        }
    }
}
