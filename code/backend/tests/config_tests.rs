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
