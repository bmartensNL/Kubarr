//! Tests for configuration module

use kubarr::config::{Config, CONFIG};

#[test]
fn test_config_defaults() {
    // Create a config with defaults (env vars not set)
    let config = Config::from_env();

    // Test default values
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8000);
    assert_eq!(config.default_namespace, "media");
    assert!(!config.oauth2_enabled);
    assert!(!config.in_cluster);
}

#[test]
fn test_database_url_format() {
    let config = Config::from_env();

    // Database URL should be a postgres URL by default
    assert!(
        config.database_url.starts_with("postgres://"),
        "Expected postgres URL, got: {}",
        config.database_url
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

    assert_eq!(config1.host, config2.host);
    assert_eq!(config1.port, config2.port);
    assert_eq!(config1.database_url, config2.database_url);
}

#[test]
fn test_config_debug() {
    let config = Config::from_env();
    let debug_str = format!("{:?}", config);

    // Debug output should contain field names
    assert!(debug_str.contains("host"));
    assert!(debug_str.contains("port"));
    assert!(debug_str.contains("database_url"));
}

#[test]
fn test_path_types() {
    let config = Config::from_env();

    // charts_dir should be PathBuf
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
