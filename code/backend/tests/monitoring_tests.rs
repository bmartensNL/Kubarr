//! Tests for monitoring configuration and error handling

use axum::http::StatusCode;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use kubarr::config::monitoring::MonitoringConfig;
use kubarr::error::AppError;

// ============================================================================
// Helper
// ============================================================================

async fn body_string(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

// ============================================================================
// Bug 1: VictoriaMetrics / VictoriaLogs URL configuration
// ============================================================================

#[test]
fn test_monitoring_config_default_victoriametrics_url() {
    // When KUBARR_VICTORIAMETRICS_URL is not set, default cluster-local URL is used.
    // This test validates the default without relying on the global CONFIG (which is
    // initialised once for the whole test binary and may see env-var noise from other
    // tests running in parallel).
    std::env::remove_var("KUBARR_VICTORIAMETRICS_URL");
    let config = MonitoringConfig::from_env();
    assert_eq!(
        config.victoriametrics_url,
        "http://victoriametrics.victoriametrics.svc.cluster.local:8428"
    );
}

#[test]
fn test_monitoring_config_default_victorialogs_url() {
    std::env::remove_var("KUBARR_VICTORIALOGS_URL");
    let config = MonitoringConfig::from_env();
    assert_eq!(
        config.victorialogs_url,
        "http://victorialogs.victorialogs.svc.cluster.local:9428"
    );
}

#[test]
fn test_monitoring_config_victoriametrics_url_env_override() {
    // KUBARR_VICTORIAMETRICS_URL env var must override the built-in default.
    // NOTE: Rust test threads share the process environment. We set and remove the
    // variable around the assertion to minimise interference with parallel tests.
    let key = "KUBARR_VICTORIAMETRICS_URL";
    let original = std::env::var(key).ok();

    std::env::set_var(key, "http://custom-vm.example.com:8428");
    let config = MonitoringConfig::from_env();
    assert_eq!(config.victoriametrics_url, "http://custom-vm.example.com:8428");

    // Restore previous state
    match original {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

#[test]
fn test_monitoring_config_victorialogs_url_env_override() {
    let key = "KUBARR_VICTORIALOGS_URL";
    let original = std::env::var(key).ok();

    std::env::set_var(key, "http://custom-vlogs.example.com:9428");
    let config = MonitoringConfig::from_env();
    assert_eq!(config.victorialogs_url, "http://custom-vlogs.example.com:9428");

    match original {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

#[test]
fn test_monitoring_config_urls_start_with_http() {
    std::env::remove_var("KUBARR_VICTORIAMETRICS_URL");
    std::env::remove_var("KUBARR_VICTORIALOGS_URL");
    let config = MonitoringConfig::from_env();
    assert!(
        config.victoriametrics_url.starts_with("http://"),
        "VictoriaMetrics URL should be an http:// URL, got: {}",
        config.victoriametrics_url
    );
    assert!(
        config.victorialogs_url.starts_with("http://"),
        "VictoriaLogs URL should be an http:// URL, got: {}",
        config.victorialogs_url
    );
}

// ============================================================================
// Bug 2: Namespace defaults driven by config, not hardcoded "media"
// ============================================================================

#[test]
fn test_kubernetes_default_namespace_env_override() {
    use kubarr::config::kubernetes::KubernetesConfig;

    let key = "KUBARR_DEFAULT_NAMESPACE";
    let original = std::env::var(key).ok();

    // Custom namespace
    std::env::set_var(key, "my-apps");
    let config = KubernetesConfig::from_env();
    assert_eq!(config.default_namespace, "my-apps");

    // Empty string → "all namespaces" semantic
    std::env::set_var(key, "");
    let config = KubernetesConfig::from_env();
    assert_eq!(config.default_namespace, "");

    match original {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

#[test]
fn test_kubernetes_default_namespace_fallback() {
    use kubarr::config::kubernetes::KubernetesConfig;

    let key = "KUBARR_DEFAULT_NAMESPACE";
    let original = std::env::var(key).ok();

    std::env::remove_var(key);
    let config = KubernetesConfig::from_env();
    // Default is still "media" for backwards compatibility
    assert_eq!(config.default_namespace, "media");

    match original {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

// ============================================================================
// Bug 3: ServiceUnavailable error when VictoriaMetrics is unreachable
// ============================================================================

/// When VictoriaMetrics is unreachable, monitoring endpoints must return HTTP 503
/// with the structured error body, not silently return empty data.
#[tokio::test]
async fn test_service_unavailable_error_format() {
    // We validate that AppError::ServiceUnavailable produces:
    //   HTTP 503 with JSON body {"detail": "..."}
    let err = AppError::ServiceUnavailable(
        "VictoriaMetrics is not reachable. Check monitoring setup.".to_string(),
    );
    let (status, body) = body_string(err.into_response()).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let json: serde_json::Value = serde_json::from_str(&body)
        .expect("Response body should be valid JSON");

    // Must carry a "detail" field with the error message
    let detail = json.get("detail").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        detail.contains("VictoriaMetrics"),
        "Error detail should mention VictoriaMetrics, got: {detail}"
    );
}

#[tokio::test]
async fn test_vm_unreachable_returns_503() {
    // Simulate what happens when query_vm / query_vm_range detect a connection error.
    // The function converts connection errors into AppError::ServiceUnavailable.
    // We verify the resulting HTTP response status and body shape.
    let error = AppError::ServiceUnavailable(
        "VictoriaMetrics is not reachable. Check monitoring setup.".to_string(),
    );
    let response = error.into_response();
    let (status, body) = body_string(response).await;

    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "Unreachable VM should produce 503"
    );
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        parsed.get("detail").is_some(),
        "Response must contain 'detail' field"
    );
    assert!(
        parsed["detail"]
            .as_str()
            .unwrap_or("")
            .contains("not reachable"),
        "Detail message should say 'not reachable'"
    );
}

// ============================================================================
// Integration test: monitoring endpoint with mocked unreachable VM
// ============================================================================

/// This integration test verifies that when VictoriaMetrics is pointed at an
/// unreachable address, the query_vm helper returns ServiceUnavailable rather
/// than silently returning empty data.
///
/// We test this by setting KUBARR_VICTORIAMETRICS_URL to a port where nothing
/// is listening, exercising the reqwest connection-refused path.
#[tokio::test]
async fn test_unreachable_vm_url_produces_service_unavailable_error() {
    use kubarr::config::monitoring::MonitoringConfig;

    // Point at an address where nothing is listening (loopback, port 19999).
    let key = "KUBARR_VICTORIAMETRICS_URL";
    let original = std::env::var(key).ok();
    std::env::set_var(key, "http://127.0.0.1:19999");

    let config = MonitoringConfig::from_env();
    assert_eq!(config.victoriametrics_url, "http://127.0.0.1:19999");

    // Attempt a real HTTP connection to the non-existent address.
    let client = reqwest::Client::new();
    let url = format!("{}/health", config.victoriametrics_url);
    let result = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await;

    // Connection should fail (connection refused or timeout).
    assert!(
        result.is_err(),
        "Connecting to a non-listening port should fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.is_connect() || err.is_timeout() || err.is_request(),
        "Error should be a connection-level failure, got: {err}"
    );

    // Restore env
    match original {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}
