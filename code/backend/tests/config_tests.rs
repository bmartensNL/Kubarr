//! Tests for configuration module

use kubarr::config::{Config, CONFIG};

#[test]
fn test_config_defaults() {
    // Create a config with defaults (env vars not set).
    // Config fields are nested: config.server.host, config.kubernetes.in_cluster, etc.
    let config = Config::from_env();

    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 8000);
    assert_eq!(config.kubernetes.default_namespace, "media");
    assert!(!config.auth.oauth2_enabled);
    assert!(!config.kubernetes.in_cluster);
}

#[test]
fn test_database_url_format() {
    let config = Config::from_env();

    // Database URL should be a postgres URL by default
    assert!(
        config.database.database_url.starts_with("postgres://"),
        "Expected postgres URL, got: {}",
        config.database.database_url
    );
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

    assert_eq!(config1.server.host, config2.server.host);
    assert_eq!(config1.server.port, config2.server.port);
    assert_eq!(
        config1.database.database_url,
        config2.database.database_url
    );
}

#[test]
fn test_config_debug() {
    let config = Config::from_env();
    let debug_str = format!("{:?}", config);

    // Debug output should contain sub-config struct names
    assert!(debug_str.contains("ServerConfig") || debug_str.contains("server"));
    assert!(debug_str.contains("DatabaseConfig") || debug_str.contains("database"));
}

#[test]
fn test_path_types() {
    let config = Config::from_env();

    // charts dir should be a valid PathBuf
    assert!(config.charts.dir.to_str().is_some());
}

#[test]
fn test_lazy_static_config() {
    // Access the global CONFIG via nested fields
    let _ = &CONFIG.server.host;
    let _ = &CONFIG.server.port;

    // CONFIG should be initialized
    assert!(!CONFIG.server.host.is_empty());
}

#[test]
fn test_allowed_origins_default_empty() {
    // When KUBARR_ALLOWED_ORIGINS is not set, the list should be empty
    // (any origin is allowed — dev convenience). This test is best-effort;
    // if the env var is set in the test environment the assertion is skipped.
    if std::env::var("KUBARR_ALLOWED_ORIGINS").is_err() {
        let config = Config::from_env();
        assert!(
            config.server.allowed_origins.is_empty(),
            "Expected empty allowed_origins when env var is unset"
        );
    }
}
