//! Tests for monitoring configuration and error handling.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use kubarr::config::Config;
use kubarr::error::AppError;

// ============================================================================
// Config helpers
// ============================================================================

async fn get_response_body(response: axum::response::Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

// ============================================================================
// Bug 1: VictoriaMetrics / VictoriaLogs URL env-var override
// ============================================================================

#[test]
fn test_victoriametrics_url_default() {
    // Without KUBARR_VICTORIAMETRICS_URL set the default cluster URL is used.
    // We read the config fresh (env var must not be set in CI for this to pass).
    let config = Config::from_env();
    assert_eq!(
        config.monitoring.victoriametrics_url,
        "http://victoriametrics.victoriametrics.svc.cluster.local:8428"
    );
}

#[test]
fn test_victorialogs_url_default() {
    let config = Config::from_env();
    assert_eq!(
        config.monitoring.victorialogs_url,
        "http://victorialogs.victorialogs.svc.cluster.local:9428"
    );
}

#[test]
fn test_victoriametrics_url_env_override() {
    // SAFETY: single-threaded test; env mutation is test-local.
    std::env::set_var("KUBARR_VICTORIAMETRICS_URL", "http://custom-vm:9999");
    let config = Config::from_env();
    std::env::remove_var("KUBARR_VICTORIAMETRICS_URL");

    assert_eq!(
        config.monitoring.victoriametrics_url,
        "http://custom-vm:9999"
    );
}

#[test]
fn test_victorialogs_url_env_override() {
    std::env::set_var("KUBARR_VICTORIALOGS_URL", "http://custom-vlogs:5555");
    let config = Config::from_env();
    std::env::remove_var("KUBARR_VICTORIALOGS_URL");

    assert_eq!(
        config.monitoring.victorialogs_url,
        "http://custom-vlogs:5555"
    );
}

// ============================================================================
// Bug 2: Namespace default
// ============================================================================

#[test]
fn test_default_namespace_empty_when_unset() {
    // When KUBARR_DEFAULT_NAMESPACE is not set the default should be "".
    std::env::remove_var("KUBARR_DEFAULT_NAMESPACE");
    let config = Config::from_env();
    assert_eq!(config.kubernetes.default_namespace, "");
}

#[test]
fn test_default_namespace_env_override() {
    std::env::set_var("KUBARR_DEFAULT_NAMESPACE", "production");
    let config = Config::from_env();
    std::env::remove_var("KUBARR_DEFAULT_NAMESPACE");

    assert_eq!(config.kubernetes.default_namespace, "production");
}

// ============================================================================
// Bug 3: ServiceUnavailable produces HTTP 503 with JSON detail
// ============================================================================

#[tokio::test]
async fn test_service_unavailable_produces_503() {
    let error = AppError::ServiceUnavailable(
        "VictoriaMetrics is not reachable. Check monitoring setup.".to_string(),
    );
    let response = error.into_response();
    let (status, body) = get_response_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("VictoriaMetrics is not reachable"));
}

#[tokio::test]
async fn test_service_unavailable_response_is_json() {
    let error = AppError::ServiceUnavailable("metrics_unavailable".to_string());
    let response = error.into_response();
    let (status, body) = get_response_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    // Response must be valid JSON with a "detail" field
    let parsed: serde_json::Value =
        serde_json::from_str(&body).expect("monitoring error response must be valid JSON");
    assert!(
        parsed.get("detail").is_some(),
        "JSON error response must have a 'detail' field"
    );
}

/// Integration test: when VictoriaMetrics is unreachable a reqwest connection
/// error can be mapped to AppError::ServiceUnavailable (HTTP 503).
#[tokio::test]
async fn test_vm_connection_error_maps_to_503() {
    // Use a port that is guaranteed to be refused.
    let unreachable_url = "http://127.0.0.1:19999/api/v1/query";

    let client = reqwest::Client::new();
    let result = client
        .get(unreachable_url)
        .query(&[("query", "up")])
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await;

    // The request must fail (connection refused or timeout).
    assert!(
        result.is_err(),
        "expected a connection error to an unused port"
    );

    // Wrap the error in AppError::ServiceUnavailable (as monitoring.rs does).
    let app_error = AppError::ServiceUnavailable(format!(
        "VictoriaMetrics is not reachable. Check monitoring setup. ({})",
        result.unwrap_err()
    ));

    let response = app_error.into_response();
    let (status, body) = get_response_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("VictoriaMetrics is not reachable"));
}
