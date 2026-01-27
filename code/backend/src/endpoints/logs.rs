use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::middleware::permissions::{Authorized, LogsView};
use crate::error::{AppError, Result};
use crate::state::AppState;

// Loki service URL inside the cluster
const LOKI_URL: &str = "http://loki.loki.svc.cluster.local:3100";

pub fn logs_routes(state: AppState) -> Router {
    Router::new()
        // Loki endpoints (must be before /:pod_name to avoid conflicts)
        .route("/loki/namespaces", get(get_loki_namespaces))
        .route("/loki/labels", get(get_loki_labels))
        .route("/loki/label/:label/values", get(get_loki_label_values))
        .route("/loki/query", get(query_loki_logs))
        // Pod logs endpoints
        .route("/raw/:pod_name", get(get_raw_pod_logs))
        .route("/app/:app_name", get(get_app_logs))
        .route("/:pod_name", get(get_pod_logs))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct LogEntry {
    timestamp: String,
    line: String,
    pod_name: String,
    container: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PodLogsQuery {
    #[serde(default = "default_namespace")]
    namespace: String,
    container: Option<String>,
    #[serde(default = "default_tail")]
    tail: i32,
}

fn default_namespace() -> String {
    "media".to_string()
}

fn default_tail() -> i32 {
    100
}

#[derive(Debug, Deserialize)]
struct LokiQueryParams {
    #[serde(default = "default_loki_query")]
    query: String,
    start: Option<String>,
    end: Option<String>,
    #[serde(default = "default_limit")]
    limit: i32,
    #[serde(default = "default_direction")]
    direction: String,
}

fn default_loki_query() -> String {
    "{namespace=~\".+\"}".to_string()
}

fn default_limit() -> i32 {
    1000
}

fn default_direction() -> String {
    "backward".to_string()
}

#[derive(Debug, Serialize)]
struct LokiQueryResponse {
    streams: Vec<LokiStream>,
    total_entries: i32,
}

#[derive(Debug, Serialize)]
struct LokiStream {
    labels: HashMap<String, String>,
    entries: Vec<LokiEntry>,
}

#[derive(Debug, Serialize)]
struct LokiEntry {
    timestamp: String,
    line: String,
}

/// Get logs from a specific pod
async fn get_pod_logs(
    State(state): State<AppState>,
    Path(pod_name): Path<String>,
    Query(params): Query<PodLogsQuery>,
    _auth: Authorized<LogsView>,
) -> Result<Json<Vec<LogEntry>>> {
    let k8s = state.k8s_client.read().await;
    let client = k8s
        .as_ref()
        .ok_or_else(|| AppError::Internal("Kubernetes client not available".to_string()))?;

    let logs = client
        .get_pod_logs(
            &pod_name,
            &params.namespace,
            params.container.as_deref(),
            params.tail,
        )
        .await?;

    let entries: Vec<LogEntry> = logs
        .lines()
        .map(|line| LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            line: line.to_string(),
            pod_name: pod_name.clone(),
            container: params.container.clone(),
        })
        .collect();

    Ok(Json(entries))
}

/// Get logs from all pods of an app
async fn get_app_logs(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    Query(params): Query<PodLogsQuery>,
    _auth: Authorized<LogsView>,
) -> Result<Json<Vec<LogEntry>>> {
    let k8s = state.k8s_client.read().await;
    let client = k8s
        .as_ref()
        .ok_or_else(|| AppError::Internal("Kubernetes client not available".to_string()))?;

    // Get all pods for the app
    let pods = client.list_pods(&params.namespace, Some(&app_name)).await?;

    let mut all_entries = Vec::new();

    for pod in pods {
        if let Some(pod_name) = pod.metadata.name {
            match client
                .get_pod_logs(
                    &pod_name,
                    &params.namespace,
                    params.container.as_deref(),
                    params.tail,
                )
                .await
            {
                Ok(logs) => {
                    for line in logs.lines() {
                        all_entries.push(LogEntry {
                            timestamp: Utc::now().to_rfc3339(),
                            line: line.to_string(),
                            pod_name: pod_name.clone(),
                            container: params.container.clone(),
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get logs for pod {}: {}", pod_name, e);
                }
            }
        }
    }

    Ok(Json(all_entries))
}

/// Get raw logs from a pod as plain text
async fn get_raw_pod_logs(
    State(state): State<AppState>,
    Path(pod_name): Path<String>,
    Query(params): Query<PodLogsQuery>,
    _auth: Authorized<LogsView>,
) -> Result<String> {
    let k8s = state.k8s_client.read().await;
    let client = k8s
        .as_ref()
        .ok_or_else(|| AppError::Internal("Kubernetes client not available".to_string()))?;

    let logs = client
        .get_pod_logs(
            &pod_name,
            &params.namespace,
            params.container.as_deref(),
            params.tail,
        )
        .await?;

    Ok(logs)
}

// ============== Loki Endpoints ==============

/// Get all namespaces that have logs in Loki
async fn get_loki_namespaces(
    _auth: Authorized<LogsView>,
) -> Result<Json<Vec<String>>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(format!("{}/loki/api/v1/label/namespace/values", LOKI_URL))
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to connect to Loki: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Loki returned error: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Loki response: {}", e)))?;

    let namespaces = data
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(namespaces))
}

/// Get all available labels from Loki
async fn get_loki_labels(
    _auth: Authorized<LogsView>,
) -> Result<Json<Vec<String>>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(format!("{}/loki/api/v1/labels", LOKI_URL))
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to connect to Loki: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Loki returned error: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Loki response: {}", e)))?;

    let labels = data
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(labels))
}

/// Get all values for a specific label from Loki
async fn get_loki_label_values(
    Path(label): Path<String>,
    _auth: Authorized<LogsView>,
) -> Result<Json<Vec<String>>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(format!("{}/loki/api/v1/label/{}/values", LOKI_URL, label))
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to connect to Loki: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Loki returned error: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Loki response: {}", e)))?;

    let values = data
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(values))
}

/// Query logs from Loki using LogQL
async fn query_loki_logs(
    Query(params): Query<LokiQueryParams>,
    _auth: Authorized<LogsView>,
) -> Result<Json<LokiQueryResponse>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    // Default to last hour if no time range specified
    let now = Utc::now();
    let end = params
        .end
        .unwrap_or_else(|| format!("{}", now.timestamp_nanos_opt().unwrap_or(0)));
    let start = params.start.unwrap_or_else(|| {
        let start_time = now - Duration::hours(1);
        format!("{}", start_time.timestamp_nanos_opt().unwrap_or(0))
    });

    let response = client
        .get(format!("{}/loki/api/v1/query_range", LOKI_URL))
        .query(&[
            ("query", params.query.as_str()),
            ("start", start.as_str()),
            ("end", end.as_str()),
            ("limit", &params.limit.to_string()),
            ("direction", params.direction.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to connect to Loki: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Loki returned error: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Loki response: {}", e)))?;

    // Parse Loki response
    let result = data
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut streams = Vec::new();
    let mut total_entries = 0;

    for stream in result {
        let labels: HashMap<String, String> = stream
            .get("stream")
            .and_then(|s| serde_json::from_value(s.clone()).ok())
            .unwrap_or_default();

        let values = stream
            .get("values")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut entries = Vec::new();

        for value in values {
            if let Some(arr) = value.as_array() {
                if arr.len() >= 2 {
                    let timestamp_ns = arr[0].as_str().unwrap_or("0");
                    let line = arr[1].as_str().unwrap_or("").to_string();

                    // Convert nanoseconds to ISO format
                    if let Ok(ns) = timestamp_ns.parse::<i64>() {
                        let secs = ns / 1_000_000_000;
                        let nsecs = (ns % 1_000_000_000) as u32;
                        if let Some(dt) = DateTime::from_timestamp(secs, nsecs) {
                            entries.push(LokiEntry {
                                timestamp: dt.to_rfc3339(),
                                line,
                            });
                            total_entries += 1;
                        }
                    }
                }
            }
        }

        streams.push(LokiStream { labels, entries });
    }

    Ok(Json(LokiQueryResponse {
        streams,
        total_entries,
    }))
}
