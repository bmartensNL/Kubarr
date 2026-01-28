use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::middleware::permissions::{Authorized, MonitoringView};
use crate::services::k8s::{PodMetrics, PodStatus, ServiceEndpoint};
use crate::state::AppState;

/// VictoriaMetrics URL (inside cluster)
const VICTORIAMETRICS_URL: &str = "http://victoriametrics.victoriametrics.svc.cluster.local:8428";

/// Create monitoring routes
pub fn monitoring_routes(state: AppState) -> Router {
    Router::new()
        .route("/vm/apps", get(get_app_metrics))
        .route("/vm/cluster", get(get_cluster_metrics))
        .route("/vm/app/:app_name", get(get_app_detail_metrics))
        .route("/vm/available", get(check_vm_available))
        .route("/pods", get(get_pods))
        .route("/metrics", get(get_metrics))
        .route("/health/:app_name", get(get_app_health))
        .route("/endpoints/:app_name", get(get_endpoints))
        .route("/metrics-available", get(check_metrics_available))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AppMetrics {
    pub app_name: String,
    pub namespace: String,
    pub cpu_usage_cores: f64,
    pub memory_usage_bytes: i64,
    pub memory_usage_mb: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_percent: Option<f64>,
    pub network_receive_bytes_per_sec: f64,
    pub network_transmit_bytes_per_sec: f64,
}

#[derive(Debug, Serialize)]
pub struct ClusterMetrics {
    pub total_cpu_cores: f64,
    pub total_memory_bytes: i64,
    pub used_cpu_cores: f64,
    pub used_memory_bytes: i64,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub container_count: i32,
    pub pod_count: i32,
    pub network_receive_bytes_per_sec: f64,
    pub network_transmit_bytes_per_sec: f64,
    pub total_storage_bytes: i64,
    pub used_storage_bytes: i64,
    pub storage_usage_percent: f64,
}

#[derive(Debug, Serialize)]
pub struct TimeSeriesPoint {
    pub timestamp: f64,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct AppHistoricalMetrics {
    pub app_name: String,
    pub namespace: String,
    pub cpu_series: Vec<TimeSeriesPoint>,
    pub memory_series: Vec<TimeSeriesPoint>,
    pub network_rx_series: Vec<TimeSeriesPoint>,
    pub network_tx_series: Vec<TimeSeriesPoint>,
    pub cpu_usage_cores: f64,
    pub memory_usage_bytes: i64,
    pub memory_usage_mb: f64,
    pub network_receive_bytes_per_sec: f64,
    pub network_transmit_bytes_per_sec: f64,
}

#[derive(Debug, Serialize)]
pub struct AppDetailMetrics {
    pub app_name: String,
    pub namespace: String,
    pub historical: AppHistoricalMetrics,
    pub pods: Vec<PodStatus>,
}

#[derive(Debug, Serialize)]
pub struct AppHealth {
    pub app_name: String,
    pub namespace: String,
    pub healthy: bool,
    pub pods: Vec<PodStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<PodMetrics>>,
    pub endpoints: Vec<ServiceEndpoint>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct PodQuery {
    pub namespace: Option<String>,
    pub app: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppDetailQuery {
    pub duration: Option<String>,
}

// ============================================================================
// VictoriaMetrics Query Helpers
// ============================================================================

async fn query_vm(query: &str) -> Vec<serde_json::Value> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/query", VICTORIAMETRICS_URL);

    match client
        .get(&url)
        .query(&[("query", query)])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if data.get("status") == Some(&serde_json::json!("success")) {
                    return data["data"]["result"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

async fn query_vm_range(query: &str, start: f64, end: f64, step: &str) -> Vec<serde_json::Value> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/query_range", VICTORIAMETRICS_URL);

    match client
        .get(&url)
        .query(&[
            ("query", query),
            ("start", &start.to_string()),
            ("end", &end.to_string()),
            ("step", step),
        ])
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if data.get("status") == Some(&serde_json::json!("success")) {
                    return data["data"]["result"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// Get resource metrics for all installed apps from VictoriaMetrics
async fn get_app_metrics(
    State(state): State<AppState>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<Vec<AppMetrics>>> {
    // Get list of known app namespaces from catalog
    let catalog = state.catalog.read().await;
    let mut allowed_namespaces: std::collections::HashSet<String> = catalog
        .get_all_apps()
        .iter()
        .map(|app| app.name.clone())
        .collect();

    // Add monitoring/system namespaces
    allowed_namespaces.insert("kubarr-system".to_string());
    allowed_namespaces.insert("victoriametrics".to_string());
    allowed_namespaces.insert("victorialogs".to_string());
    allowed_namespaces.insert("fluent-bit".to_string());
    allowed_namespaces.insert("grafana".to_string());

    // Query CPU usage by namespace
    let cpu_query = r#"sum by (namespace) (rate(container_cpu_usage_seconds_total{container!="",container!="POD"}[5m]))"#;
    let cpu_results = query_vm(cpu_query).await;

    // Query memory usage by namespace
    let memory_query = r#"sum by (namespace) (container_memory_working_set_bytes{container!="",container!="POD"})"#;
    let memory_results = query_vm(memory_query).await;

    // Query network receive rate by namespace
    let network_rx_query =
        r#"sum by (namespace) (rate(container_network_receive_bytes_total{interface!="lo"}[5m]))"#;
    let network_rx_results = query_vm(network_rx_query).await;

    // Query network transmit rate by namespace
    let network_tx_query =
        r#"sum by (namespace) (rate(container_network_transmit_bytes_total{interface!="lo"}[5m]))"#;
    let network_tx_results = query_vm(network_tx_query).await;

    let mut metrics_map = std::collections::HashMap::new();

    // Process CPU results
    for result in &cpu_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            // Only include namespaces in our allowed list
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let cpu_val: f64 = value.parse().unwrap_or(0.0);
                metrics_map.insert(
                    namespace.to_string(),
                    AppMetrics {
                        app_name: namespace.to_string(),
                        namespace: namespace.to_string(),
                        cpu_usage_cores: (cpu_val * 10000.0).round() / 10000.0,
                        memory_usage_bytes: 0,
                        memory_usage_mb: 0.0,
                        cpu_usage_percent: None,
                        memory_usage_percent: None,
                        network_receive_bytes_per_sec: 0.0,
                        network_transmit_bytes_per_sec: 0.0,
                    },
                );
            }
        }
    }

    // Process memory results
    for result in &memory_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            // Only include namespaces in our allowed list
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let mem_val: i64 = value.parse::<f64>().unwrap_or(0.0) as i64;
                if let Some(metrics) = metrics_map.get_mut(namespace) {
                    metrics.memory_usage_bytes = mem_val;
                    metrics.memory_usage_mb =
                        (mem_val as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0;
                } else {
                    metrics_map.insert(
                        namespace.to_string(),
                        AppMetrics {
                            app_name: namespace.to_string(),
                            namespace: namespace.to_string(),
                            cpu_usage_cores: 0.0,
                            memory_usage_bytes: mem_val,
                            memory_usage_mb: (mem_val as f64 / (1024.0 * 1024.0) * 100.0).round()
                                / 100.0,
                            cpu_usage_percent: None,
                            memory_usage_percent: None,
                            network_receive_bytes_per_sec: 0.0,
                            network_transmit_bytes_per_sec: 0.0,
                        },
                    );
                }
            }
        }
    }

    // Process network receive results
    for result in &network_rx_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let rx_val: f64 = value.parse().unwrap_or(0.0);
                if let Some(metrics) = metrics_map.get_mut(namespace) {
                    metrics.network_receive_bytes_per_sec = (rx_val * 100.0).round() / 100.0;
                }
            }
        }
    }

    // Process network transmit results
    for result in &network_tx_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let tx_val: f64 = value.parse().unwrap_or(0.0);
                if let Some(metrics) = metrics_map.get_mut(namespace) {
                    metrics.network_transmit_bytes_per_sec = (tx_val * 100.0).round() / 100.0;
                }
            }
        }
    }

    Ok(Json(metrics_map.into_values().collect()))
}

/// Get overall cluster resource metrics from VictoriaMetrics
async fn get_cluster_metrics(_auth: Authorized<MonitoringView>) -> Result<Json<ClusterMetrics>> {
    // Total CPU cores
    let total_cpu = query_vm("sum(machine_cpu_cores)")
        .await
        .first()
        .and_then(|r| r["value"][1].as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Total memory
    let total_memory = query_vm("sum(machine_memory_bytes)")
        .await
        .first()
        .and_then(|r| r["value"][1].as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as i64;

    // Used CPU
    let used_cpu = query_vm(
        r#"sum(rate(container_cpu_usage_seconds_total{container!="",container!="POD"}[5m]))"#,
    )
    .await
    .first()
    .and_then(|r| r["value"][1].as_str())
    .and_then(|v| v.parse::<f64>().ok())
    .unwrap_or(0.0);

    // Used memory
    let used_memory =
        query_vm(r#"sum(container_memory_working_set_bytes{container!="",container!="POD"})"#)
            .await
            .first()
            .and_then(|r| r["value"][1].as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0) as i64;

    // Container count
    let container_count = query_vm(r#"count(container_last_seen{container!="",container!="POD"})"#)
        .await
        .first()
        .and_then(|r| r["value"][1].as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as i32;

    // Pod count
    let pod_count = query_vm(
        r#"count(count by (pod, namespace) (container_last_seen{container!="",container!="POD"}))"#,
    )
    .await
    .first()
    .and_then(|r| r["value"][1].as_str())
    .and_then(|v| v.parse::<f64>().ok())
    .unwrap_or(0.0) as i32;

    // Network receive rate
    let network_rx =
        query_vm(r#"sum(rate(container_network_receive_bytes_total{interface!="lo"}[5m]))"#)
            .await
            .first()
            .and_then(|r| r["value"][1].as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

    // Network transmit rate
    let network_tx =
        query_vm(r#"sum(rate(container_network_transmit_bytes_total{interface!="lo"}[5m]))"#)
            .await
            .first()
            .and_then(|r| r["value"][1].as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

    // Storage metrics
    let total_storage = query_vm(r#"max(container_fs_limit_bytes{id="/",device=~"/dev/.*"})"#)
        .await
        .first()
        .and_then(|r| r["value"][1].as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as i64;

    let used_storage = query_vm(r#"max(container_fs_usage_bytes{id="/",device=~"/dev/.*"})"#)
        .await
        .first()
        .and_then(|r| r["value"][1].as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as i64;

    Ok(Json(ClusterMetrics {
        total_cpu_cores: (total_cpu * 100.0).round() / 100.0,
        total_memory_bytes: total_memory,
        used_cpu_cores: (used_cpu * 10000.0).round() / 10000.0,
        used_memory_bytes: used_memory,
        cpu_usage_percent: if total_cpu > 0.0 {
            (used_cpu / total_cpu * 10000.0).round() / 100.0
        } else {
            0.0
        },
        memory_usage_percent: if total_memory > 0 {
            (used_memory as f64 / total_memory as f64 * 10000.0).round() / 100.0
        } else {
            0.0
        },
        container_count,
        pod_count,
        network_receive_bytes_per_sec: (network_rx * 100.0).round() / 100.0,
        network_transmit_bytes_per_sec: (network_tx * 100.0).round() / 100.0,
        total_storage_bytes: total_storage,
        used_storage_bytes: used_storage,
        storage_usage_percent: if total_storage > 0 {
            (used_storage as f64 / total_storage as f64 * 10000.0).round() / 100.0
        } else {
            0.0
        },
    }))
}

/// Get detailed metrics for a specific app
async fn get_app_detail_metrics(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    Query(query): Query<AppDetailQuery>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<AppDetailMetrics>> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = query.duration.unwrap_or_else(|| "1h".to_string());

    // Parse duration to seconds
    let duration_seconds: i64 = match duration.as_str() {
        "15m" => 15 * 60,
        "1h" => 60 * 60,
        "3h" => 3 * 60 * 60,
        "6h" => 6 * 60 * 60,
        "12h" => 12 * 60 * 60,
        "24h" => 24 * 60 * 60,
        _ => 3600,
    };

    let end_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let start_time = end_time - duration_seconds as f64;

    let step = if duration_seconds <= 3600 {
        "60s"
    } else if duration_seconds <= 21600 {
        "120s"
    } else {
        "300s"
    };

    // Build VictoriaMetrics queries by namespace
    let cpu_query = format!(
        r#"sum(rate(container_cpu_usage_seconds_total{{namespace="{}",container!="",container!="POD"}}[5m]))"#,
        app_name
    );
    let memory_query = format!(
        r#"sum(container_memory_working_set_bytes{{namespace="{}",container!="",container!="POD"}})"#,
        app_name
    );
    let network_rx_query = format!(
        r#"sum(rate(container_network_receive_bytes_total{{namespace="{}",interface!="lo"}}[5m]))"#,
        app_name
    );
    let network_tx_query = format!(
        r#"sum(rate(container_network_transmit_bytes_total{{namespace="{}",interface!="lo"}}[5m]))"#,
        app_name
    );

    // Query historical CPU
    let cpu_results = query_vm_range(&cpu_query, start_time, end_time, step).await;

    let cpu_series: Vec<TimeSeriesPoint> = cpu_results
        .first()
        .and_then(|r| r["values"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    let ts = v[0].as_f64()?;
                    let val: f64 = v[1].as_str()?.parse().ok()?;
                    Some(TimeSeriesPoint {
                        timestamp: ts,
                        value: val,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Query historical memory
    let memory_results = query_vm_range(&memory_query, start_time, end_time, step).await;

    let memory_series: Vec<TimeSeriesPoint> = memory_results
        .first()
        .and_then(|r| r["values"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    let ts = v[0].as_f64()?;
                    let val: f64 = v[1].as_str()?.parse().ok()?;
                    Some(TimeSeriesPoint {
                        timestamp: ts,
                        value: val,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Query historical network receive
    let network_rx_results = query_vm_range(&network_rx_query, start_time, end_time, step).await;

    let network_rx_series: Vec<TimeSeriesPoint> = network_rx_results
        .first()
        .and_then(|r| r["values"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    let ts = v[0].as_f64()?;
                    let val: f64 = v[1].as_str()?.parse().ok()?;
                    Some(TimeSeriesPoint {
                        timestamp: ts,
                        value: val,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Query historical network transmit
    let network_tx_results = query_vm_range(&network_tx_query, start_time, end_time, step).await;

    let network_tx_series: Vec<TimeSeriesPoint> = network_tx_results
        .first()
        .and_then(|r| r["values"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    let ts = v[0].as_f64()?;
                    let val: f64 = v[1].as_str()?.parse().ok()?;
                    Some(TimeSeriesPoint {
                        timestamp: ts,
                        value: val,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let current_cpu = cpu_series.last().map(|p| p.value).unwrap_or(0.0);
    let current_memory = memory_series.last().map(|p| p.value as i64).unwrap_or(0);
    let current_network_rx = network_rx_series.last().map(|p| p.value).unwrap_or(0.0);
    let current_network_tx = network_tx_series.last().map(|p| p.value).unwrap_or(0.0);

    // Get pod status
    let mut pods = if let Some(client) = state.k8s_client.read().await.as_ref() {
        client
            .get_pod_status(&app_name, None)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Query per-pod CPU and memory metrics from VictoriaMetrics
    let pod_cpu_query = format!(
        r#"sum(rate(container_cpu_usage_seconds_total{{namespace="{}",container!="",container!="POD"}}[5m])) by (pod)"#,
        app_name
    );
    let pod_memory_query = format!(
        r#"sum(container_memory_working_set_bytes{{namespace="{}",container!="",container!="POD"}}) by (pod)"#,
        app_name
    );

    let pod_cpu_results = query_vm(&pod_cpu_query).await;
    let pod_memory_results = query_vm(&pod_memory_query).await;

    // Build maps of pod name -> metric value
    let mut pod_cpu_map: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut pod_memory_map: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();

    for result in &pod_cpu_results {
        if let (Some(pod_name), Some(value_str)) = (
            result["metric"]["pod"].as_str(),
            result["value"]
                .as_array()
                .and_then(|v| v.get(1))
                .and_then(|v| v.as_str()),
        ) {
            if let Ok(value) = value_str.parse::<f64>() {
                pod_cpu_map.insert(pod_name.to_string(), value);
            }
        }
    }

    for result in &pod_memory_results {
        if let (Some(pod_name), Some(value_str)) = (
            result["metric"]["pod"].as_str(),
            result["value"]
                .as_array()
                .and_then(|v| v.get(1))
                .and_then(|v| v.as_str()),
        ) {
            if let Ok(value) = value_str.parse::<f64>() {
                pod_memory_map.insert(pod_name.to_string(), value as i64);
            }
        }
    }

    // Merge metrics into pod status
    for pod in &mut pods {
        pod.cpu_usage = pod_cpu_map.get(&pod.name).copied();
        pod.memory_usage = pod_memory_map.get(&pod.name).copied();
    }

    Ok(Json(AppDetailMetrics {
        app_name: app_name.clone(),
        namespace: app_name.clone(),
        historical: AppHistoricalMetrics {
            app_name: app_name.clone(),
            namespace: app_name.clone(),
            cpu_series,
            memory_series,
            network_rx_series,
            network_tx_series,
            cpu_usage_cores: (current_cpu * 10000.0).round() / 10000.0,
            memory_usage_bytes: current_memory,
            memory_usage_mb: (current_memory as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0,
            network_receive_bytes_per_sec: (current_network_rx * 100.0).round() / 100.0,
            network_transmit_bytes_per_sec: (current_network_tx * 100.0).round() / 100.0,
        },
        pods,
    }))
}

/// Check if VictoriaMetrics is available
async fn check_vm_available(_auth: Authorized<MonitoringView>) -> Result<Json<serde_json::Value>> {
    let client = reqwest::Client::new();
    // VictoriaMetrics uses /health endpoint for health checks
    let url = format!("{}/health", VICTORIAMETRICS_URL);

    let available = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    Ok(Json(serde_json::json!({
        "available": available,
        "message": if available { "VictoriaMetrics is available" } else { "Cannot connect to VictoriaMetrics" }
    })))
}

/// Get pod status
async fn get_pods(
    State(state): State<AppState>,
    Query(query): Query<PodQuery>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<Vec<PodStatus>>> {
    let namespace = query.namespace.unwrap_or_else(|| "media".to_string());

    let pods = if let Some(client) = state.k8s_client.read().await.as_ref() {
        client
            .get_pod_status(&namespace, query.app.as_deref())
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Json(pods))
}

/// Get pod metrics
async fn get_metrics(
    State(state): State<AppState>,
    Query(query): Query<PodQuery>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<Vec<PodMetrics>>> {
    let namespace = query.namespace.unwrap_or_else(|| "media".to_string());

    let metrics = if let Some(client) = state.k8s_client.read().await.as_ref() {
        client
            .get_pod_metrics(&namespace, query.app.as_deref())
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Json(metrics))
}

/// Get app health
async fn get_app_health(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    Query(query): Query<PodQuery>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<AppHealth>> {
    let namespace = query.namespace.unwrap_or_else(|| "media".to_string());

    let (pods, metrics, endpoints) = if let Some(client) = state.k8s_client.read().await.as_ref() {
        let pods = client
            .get_pod_status(&namespace, Some(&app_name))
            .await
            .unwrap_or_default();
        let metrics = client
            .get_pod_metrics(&namespace, Some(&app_name))
            .await
            .ok();
        let endpoints = client
            .get_service_endpoints(&app_name, &namespace)
            .await
            .unwrap_or_default();
        (pods, metrics, endpoints)
    } else {
        (Vec::new(), None, Vec::new())
    };

    // Determine health
    let (healthy, message) = if pods.is_empty() {
        (false, "No pods found".to_string())
    } else {
        let running_ready: Vec<_> = pods
            .iter()
            .filter(|p| p.status == "Running" && p.ready)
            .collect();

        if running_ready.len() != pods.len() {
            (
                false,
                format!("{}/{} pods ready", running_ready.len(), pods.len()),
            )
        } else {
            let high_restarts: Vec<_> = pods.iter().filter(|p| p.restart_count > 5).collect();
            if !high_restarts.is_empty() {
                (false, "Pods restarting frequently".to_string())
            } else {
                (true, "All pods running".to_string())
            }
        }
    };

    Ok(Json(AppHealth {
        app_name: app_name.clone(),
        namespace,
        healthy,
        pods,
        metrics,
        endpoints,
        message,
    }))
}

/// Get service endpoints
async fn get_endpoints(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    Query(query): Query<PodQuery>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<Vec<ServiceEndpoint>>> {
    let namespace = query.namespace.unwrap_or_else(|| "media".to_string());

    let endpoints = if let Some(client) = state.k8s_client.read().await.as_ref() {
        client
            .get_service_endpoints(&app_name, &namespace)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Json(endpoints))
}

/// Check if metrics-server is available
async fn check_metrics_available(
    State(state): State<AppState>,
    _auth: Authorized<MonitoringView>,
) -> Result<Json<serde_json::Value>> {
    let available = if let Some(client) = state.k8s_client.read().await.as_ref() {
        client.check_metrics_server_available().await
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "available": available,
        "message": if available { "Metrics server is available" } else { "Metrics server not found" }
    })))
}
