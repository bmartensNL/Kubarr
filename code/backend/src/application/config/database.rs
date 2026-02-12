use std::env;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub database_url: String,
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("KUBARR_DATABASE_URL")
                .or_else(|_| env::var("DATABASE_URL"))
                .unwrap_or_else(|_| "postgres://kubarr:kubarr@localhost:5432/kubarr".to_string()),
        }
    }
}
