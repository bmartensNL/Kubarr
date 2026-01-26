use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::api::extractors::AuthUser;
use crate::error::Result;
use crate::state::AppState;

/// VictoriaMetrics URL (inside cluster)
const VICTORIAMETRICS_URL: &str = "http://victoriametrics.victoriametrics.svc.cluster.local:8428";

/// Create networking routes
pub fn networking_routes(state: AppState) -> Router {
    Router::new()
        .route("/topology", get(get_network_topology))
        .route("/stats", get(get_network_stats))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct NetworkNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub total_traffic: f64,
    pub pod_count: i32,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkEdge {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct NetworkTopology {
    pub nodes: Vec<NetworkNode>,
    pub edges: Vec<NetworkEdge>,
}

#[derive(Debug, Serialize)]
pub struct NetworkStats {
    pub namespace: String,
    pub app_name: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_packets_per_sec: f64,
    pub tx_packets_per_sec: f64,
    pub rx_errors_per_sec: f64,
    pub tx_errors_per_sec: f64,
    pub rx_dropped_per_sec: f64,
    pub tx_dropped_per_sec: f64,
    pub pod_count: i32,
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

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// Get network topology with nodes and edges
async fn get_network_topology(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<NetworkTopology>> {
    // Get list of known app namespaces from catalog
    let catalog = state.catalog.read().await;
    let mut allowed_namespaces: HashSet<String> = catalog
        .get_all_apps()
        .iter()
        .map(|app| app.name.clone())
        .collect();

    // Add monitoring/system namespaces
    allowed_namespaces.insert("kubarr-system".to_string());
    allowed_namespaces.insert("victoriametrics".to_string());
    allowed_namespaces.insert("loki".to_string());
    allowed_namespaces.insert("grafana".to_string());

    // Query network metrics from VictoriaMetrics
    let rx_query =
        r#"sum by (namespace) (rate(container_network_receive_bytes_total{interface!="lo"}[5m]))"#;
    let tx_query =
        r#"sum by (namespace) (rate(container_network_transmit_bytes_total{interface!="lo"}[5m]))"#;
    let pod_count_query = r#"count by (namespace) (kube_pod_info)"#;

    let rx_results = query_vm(rx_query).await;
    let tx_results = query_vm(tx_query).await;
    let pod_results = query_vm(pod_count_query).await;

    // Build namespace metrics map
    let mut metrics_map: HashMap<String, (f64, f64, i32)> = HashMap::new();

    // Process RX results
    for result in &rx_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let rx_val: f64 = value.parse().unwrap_or(0.0);
                metrics_map
                    .entry(namespace.to_string())
                    .or_insert((0.0, 0.0, 1))
                    .0 = rx_val;
            }
        }
    }

    // Process TX results
    for result in &tx_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let tx_val: f64 = value.parse().unwrap_or(0.0);
                metrics_map
                    .entry(namespace.to_string())
                    .or_insert((0.0, 0.0, 1))
                    .1 = tx_val;
            }
        }
    }

    // Process pod count results
    for result in &pod_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            if !allowed_namespaces.contains(namespace) {
                continue;
            }
            if let Some(value) = result["value"][1].as_str() {
                let count: i32 = value.parse().unwrap_or(1);
                if let Some(entry) = metrics_map.get_mut(namespace) {
                    entry.2 = count;
                }
            }
        }
    }

    // Color palette for nodes
    let colors = [
        "#3b82f6", // blue
        "#22c55e", // green
        "#f59e0b", // amber
        "#ef4444", // red
        "#8b5cf6", // violet
        "#06b6d4", // cyan
        "#ec4899", // pink
        "#f97316", // orange
    ];

    // Build nodes
    let mut nodes: Vec<NetworkNode> = Vec::new();
    for (i, (namespace, (rx, tx, pods))) in metrics_map.iter().enumerate() {
        let node_type = if namespace == "kubarr-system" {
            "system"
        } else if namespace == "victoriametrics" || namespace == "loki" || namespace == "grafana" {
            "monitoring"
        } else {
            "app"
        };

        nodes.push(NetworkNode {
            id: namespace.clone(),
            name: capitalize_first(namespace),
            node_type: node_type.to_string(),
            rx_bytes_per_sec: (*rx * 100.0).round() / 100.0,
            tx_bytes_per_sec: (*tx * 100.0).round() / 100.0,
            total_traffic: ((*rx + *tx) * 100.0).round() / 100.0,
            pod_count: *pods,
            color: colors[i % colors.len()].to_string(),
        });
    }

    // Add external/internet node if there's significant traffic
    let total_rx: f64 = nodes.iter().map(|n| n.rx_bytes_per_sec).sum();
    let total_tx: f64 = nodes.iter().map(|n| n.tx_bytes_per_sec).sum();
    if total_rx > 0.0 || total_tx > 0.0 {
        nodes.push(NetworkNode {
            id: "external".to_string(),
            name: "Internet".to_string(),
            node_type: "external".to_string(),
            rx_bytes_per_sec: total_tx, // External receives what we transmit
            tx_bytes_per_sec: total_rx, // External transmits what we receive
            total_traffic: total_rx + total_tx,
            pod_count: 0,
            color: "#6b7280".to_string(), // gray
        });
    }

    // Infer edges from Kubernetes Services
    let mut edges: Vec<NetworkEdge> = Vec::new();

    if let Some(client) = state.k8s_client.read().await.as_ref() {
        // Get all services across namespaces we care about
        for namespace in &allowed_namespaces {
            if let Ok(services) = list_services(client, namespace).await {
                for svc in services {
                    let svc_name = svc.metadata.name.clone().unwrap_or_default();
                    let svc_namespace = namespace.clone();

                    // Get service ports
                    let ports: Vec<i32> = svc
                        .spec
                        .as_ref()
                        .and_then(|s| s.ports.as_ref())
                        .map(|p| p.iter().map(|port| port.port).collect())
                        .unwrap_or_default();

                    let port = ports.first().copied();

                    // Check service selector to see what it connects to
                    let selector = svc
                        .spec
                        .as_ref()
                        .and_then(|s| s.selector.as_ref())
                        .cloned()
                        .unwrap_or_default();

                    // If the service exists, infer connections from other namespaces
                    // For now, connect apps to kubarr-system (dashboard)
                    if svc_namespace != "kubarr-system" && metrics_map.contains_key(&svc_namespace)
                    {
                        edges.push(NetworkEdge {
                            source: "kubarr-system".to_string(),
                            target: svc_namespace.clone(),
                            edge_type: "service".to_string(),
                            port,
                            protocol: Some("HTTP".to_string()),
                            label: format!("{}", port.map(|p| p.to_string()).unwrap_or_default()),
                        });
                    }
                }
            }
        }
    }

    // Add external edges for apps with traffic
    for node in &nodes {
        if node.node_type == "app" && node.total_traffic > 1000.0 {
            edges.push(NetworkEdge {
                source: node.id.clone(),
                target: "external".to_string(),
                edge_type: "external".to_string(),
                port: None,
                protocol: None,
                label: "".to_string(),
            });
        }
    }

    // Add known service dependencies based on app types
    add_known_dependencies(&mut edges, &metrics_map);

    // Deduplicate edges
    edges.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));
    edges.dedup_by(|a, b| a.source == b.source && a.target == b.target);

    Ok(Json(NetworkTopology { nodes, edges }))
}

/// Get detailed network statistics per app
async fn get_network_stats(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<NetworkStats>>> {
    // Get list of known app namespaces
    let catalog = state.catalog.read().await;
    let mut allowed_namespaces: HashSet<String> = catalog
        .get_all_apps()
        .iter()
        .map(|app| app.name.clone())
        .collect();

    allowed_namespaces.insert("kubarr-system".to_string());
    allowed_namespaces.insert("victoriametrics".to_string());
    allowed_namespaces.insert("loki".to_string());
    allowed_namespaces.insert("grafana".to_string());

    // Query all network metrics
    let queries = [
        (
            r#"sum by (namespace) (rate(container_network_receive_bytes_total{interface!="lo"}[5m]))"#,
            "rx_bytes",
        ),
        (
            r#"sum by (namespace) (rate(container_network_transmit_bytes_total{interface!="lo"}[5m]))"#,
            "tx_bytes",
        ),
        (
            r#"sum by (namespace) (rate(container_network_receive_packets_total{interface!="lo"}[5m]))"#,
            "rx_packets",
        ),
        (
            r#"sum by (namespace) (rate(container_network_transmit_packets_total{interface!="lo"}[5m]))"#,
            "tx_packets",
        ),
        (
            r#"sum by (namespace) (rate(container_network_receive_errors_total{interface!="lo"}[5m]))"#,
            "rx_errors",
        ),
        (
            r#"sum by (namespace) (rate(container_network_transmit_errors_total{interface!="lo"}[5m]))"#,
            "tx_errors",
        ),
        (
            r#"sum by (namespace) (rate(container_network_receive_packets_dropped_total{interface!="lo"}[5m]))"#,
            "rx_dropped",
        ),
        (
            r#"sum by (namespace) (rate(container_network_transmit_packets_dropped_total{interface!="lo"}[5m]))"#,
            "tx_dropped",
        ),
        (r#"count by (namespace) (kube_pod_info)"#, "pod_count"),
    ];

    let mut metrics_map: HashMap<String, HashMap<String, f64>> = HashMap::new();

    for (query, metric_name) in queries {
        let results = query_vm(query).await;
        for result in results {
            if let Some(namespace) = result["metric"]["namespace"].as_str() {
                if !allowed_namespaces.contains(namespace) {
                    continue;
                }
                if let Some(value) = result["value"][1].as_str() {
                    let val: f64 = value.parse().unwrap_or(0.0);
                    metrics_map
                        .entry(namespace.to_string())
                        .or_insert_with(HashMap::new)
                        .insert(metric_name.to_string(), val);
                }
            }
        }
    }

    // Build stats list
    let stats: Vec<NetworkStats> = metrics_map
        .into_iter()
        .map(|(namespace, metrics)| NetworkStats {
            namespace: namespace.clone(),
            app_name: capitalize_first(&namespace),
            rx_bytes_per_sec: (metrics.get("rx_bytes").unwrap_or(&0.0) * 100.0).round() / 100.0,
            tx_bytes_per_sec: (metrics.get("tx_bytes").unwrap_or(&0.0) * 100.0).round() / 100.0,
            rx_packets_per_sec: (metrics.get("rx_packets").unwrap_or(&0.0) * 100.0).round() / 100.0,
            tx_packets_per_sec: (metrics.get("tx_packets").unwrap_or(&0.0) * 100.0).round() / 100.0,
            rx_errors_per_sec: (metrics.get("rx_errors").unwrap_or(&0.0) * 100.0).round() / 100.0,
            tx_errors_per_sec: (metrics.get("tx_errors").unwrap_or(&0.0) * 100.0).round() / 100.0,
            rx_dropped_per_sec: (metrics.get("rx_dropped").unwrap_or(&0.0) * 100.0).round() / 100.0,
            tx_dropped_per_sec: (metrics.get("tx_dropped").unwrap_or(&0.0) * 100.0).round() / 100.0,
            pod_count: metrics.get("pod_count").unwrap_or(&1.0).round() as i32,
        })
        .collect();

    Ok(Json(stats))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// List services in a namespace
async fn list_services(
    client: &crate::services::K8sClient,
    namespace: &str,
) -> Result<Vec<k8s_openapi::api::core::v1::Service>> {
    use k8s_openapi::api::core::v1::Service;
    use kube::api::{Api, ListParams};

    let services: Api<Service> = Api::namespaced(client.client().clone(), namespace);
    let svc_list = services.list(&ListParams::default()).await?;
    Ok(svc_list.items)
}

/// Capitalize first letter
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Add known dependencies based on common media server patterns
fn add_known_dependencies(
    edges: &mut Vec<NetworkEdge>,
    namespaces: &HashMap<String, (f64, f64, i32)>,
) {
    // Define known service relationships
    let relationships = [
        // Sonarr/Radarr typically connect to download clients
        ("sonarr", "qbittorrent", "BitTorrent", 8080),
        ("sonarr", "sabnzbd", "Usenet", 8080),
        ("sonarr", "nzbget", "Usenet", 6789),
        ("radarr", "qbittorrent", "BitTorrent", 8080),
        ("radarr", "sabnzbd", "Usenet", 8080),
        ("radarr", "nzbget", "Usenet", 6789),
        // Overseerr connects to Sonarr/Radarr
        ("overseerr", "sonarr", "API", 8989),
        ("overseerr", "radarr", "API", 7878),
        // Plex/Jellyfin are media servers
        ("overseerr", "plex", "API", 32400),
        ("overseerr", "jellyfin", "API", 8096),
        // Prowlarr provides indexers
        ("sonarr", "prowlarr", "Indexers", 9696),
        ("radarr", "prowlarr", "Indexers", 9696),
        // VictoriaMetrics scrapes everything
        ("victoriametrics", "sonarr", "Metrics", 8989),
        ("victoriametrics", "radarr", "Metrics", 7878),
        ("victoriametrics", "qbittorrent", "Metrics", 8080),
        ("victoriametrics", "kubarr-system", "Metrics", 8000),
    ];

    for (source, target, protocol, port) in relationships {
        if namespaces.contains_key(source) && namespaces.contains_key(target) {
            edges.push(NetworkEdge {
                source: source.to_string(),
                target: target.to_string(),
                edge_type: "service".to_string(),
                port: Some(port),
                protocol: Some(protocol.to_string()),
                label: protocol.to_string(),
            });
        }
    }
}
