use once_cell::sync::Lazy;
use std::env;
use std::path::PathBuf;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    // Server
    pub host: String,
    pub port: u16,

    // Database
    pub db_path: PathBuf,

    // Kubernetes
    pub kubeconfig_path: Option<PathBuf>,
    pub in_cluster: bool,
    pub default_namespace: String,

    // JWT/OAuth2
    pub jwt_private_key_path: PathBuf,
    pub jwt_public_key_path: PathBuf,
    pub jwt_algorithm: String,
    pub oauth2_enabled: bool,
    pub oauth2_issuer_url: String,

    // Static files
    pub static_files_dir: PathBuf,
    pub charts_dir: PathBuf,

    // Build info
    pub commit_hash: String,
    pub build_time: String,
    pub version: String,

    // Logging
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            // Server
            host: env::var("KUBARR_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("KUBARR_API_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8000),

            // Database
            db_path: PathBuf::from(
                env::var("KUBARR_DB_PATH").unwrap_or_else(|_| "/data/kubarr.db".to_string()),
            ),

            // Kubernetes
            kubeconfig_path: env::var("KUBARR_KUBECONFIG_PATH").ok().map(PathBuf::from),
            in_cluster: env::var("KUBARR_IN_CLUSTER")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            default_namespace: env::var("KUBARR_DEFAULT_NAMESPACE")
                .unwrap_or_else(|_| "media".to_string()),

            // JWT/OAuth2
            jwt_private_key_path: PathBuf::from(
                env::var("KUBARR_JWT_PRIVATE_KEY_PATH")
                    .unwrap_or_else(|_| "/secrets/jwt-private.pem".to_string()),
            ),
            jwt_public_key_path: PathBuf::from(
                env::var("KUBARR_JWT_PUBLIC_KEY_PATH")
                    .unwrap_or_else(|_| "/secrets/jwt-public.pem".to_string()),
            ),
            jwt_algorithm: env::var("KUBARR_JWT_ALGORITHM").unwrap_or_else(|_| "RS256".to_string()),
            oauth2_enabled: env::var("KUBARR_OAUTH2_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            oauth2_issuer_url: env::var("KUBARR_OAUTH2_ISSUER_URL")
                .unwrap_or_else(|_| "http://kubarr:8000/auth".to_string()),

            // Static files
            static_files_dir: PathBuf::from(
                env::var("STATIC_FILES_DIR").unwrap_or_else(|_| "/app/static".to_string()),
            ),
            charts_dir: PathBuf::from(
                env::var("KUBARR_CHARTS_DIR").unwrap_or_else(|_| "/app/charts".to_string()),
            ),

            // Build info
            commit_hash: env::var("COMMIT_HASH").unwrap_or_else(|_| "unknown".to_string()),
            build_time: env::var("BUILD_TIME").unwrap_or_else(|_| "unknown".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),

            // Logging
            log_level: env::var("KUBARR_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        }
    }

    pub fn db_url(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.db_path.display())
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(Config::from_env);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        // Create a config with defaults (env vars not set)
        let config = Config::from_env();

        // Test default values
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8000);
        assert_eq!(config.default_namespace, "media");
        assert_eq!(config.jwt_algorithm, "RS256");
        assert!(!config.oauth2_enabled);
        assert!(!config.in_cluster);
    }

    #[test]
    fn test_db_url_format() {
        let config = Config::from_env();
        let db_url = config.db_url();

        assert!(db_url.starts_with("sqlite://"));
        assert!(db_url.contains("?mode=rwc"));
    }

    #[test]
    fn test_version_from_cargo() {
        let config = Config::from_env();
        // Version should be set from Cargo.toml
        assert!(!config.version.is_empty());
        assert!(config.version.contains('.'));
    }

    #[test]
    fn test_config_clone() {
        let config1 = Config::from_env();
        let config2 = config1.clone();

        assert_eq!(config1.host, config2.host);
        assert_eq!(config1.port, config2.port);
        assert_eq!(config1.db_path, config2.db_path);
    }

    #[test]
    fn test_config_debug() {
        let config = Config::from_env();
        let debug_str = format!("{:?}", config);

        // Debug output should contain field names
        assert!(debug_str.contains("host"));
        assert!(debug_str.contains("port"));
        assert!(debug_str.contains("db_path"));
    }

    #[test]
    fn test_path_types() {
        let config = Config::from_env();

        // All path fields should be PathBuf
        assert!(config.db_path.to_str().is_some());
        assert!(config.jwt_private_key_path.to_str().is_some());
        assert!(config.jwt_public_key_path.to_str().is_some());
        assert!(config.static_files_dir.to_str().is_some());
        assert!(config.charts_dir.to_str().is_some());
    }

    #[test]
    fn test_lazy_static_config() {
        // Access the global CONFIG
        let _ = &CONFIG.host;
        let _ = &CONFIG.port;

        // CONFIG should be initialized
        assert!(!CONFIG.host.is_empty());
    }
}
