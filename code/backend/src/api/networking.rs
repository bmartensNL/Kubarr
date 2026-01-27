use axum::{extract::{Extension, State}, routing::get, Json, Router};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::api::middleware::AuthenticatedUser;
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
    Extension(_auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<NetworkTopology>> {
    // Query network metrics from VictoriaMetrics - get ALL namespaces
    let rx_query =
        r#"sum by (namespace) (rate(container_network_receive_bytes_total{interface!="lo"}[5m]))"#;
    let tx_query =
        r#"sum by (namespace) (rate(container_network_transmit_bytes_total{interface!="lo"}[5m]))"#;
    let pod_count_query = r#"count by (namespace) (kube_pod_info)"#;

    let rx_results = query_vm(rx_query).await;
    let tx_results = query_vm(tx_query).await;
    let pod_results = query_vm(pod_count_query).await;

    // Build namespace metrics map - include ALL namespaces
    let mut metrics_map: HashMap<String, (f64, f64, i32)> = HashMap::new();

    // Process RX results
    for result in &rx_results {
        if let Some(namespace) = result["metric"]["namespace"].as_str() {
            // Skip kube-system and other internal k8s namespaces
            if namespace.starts_with("kube-")
                || namespace == "local-path-storage"
                || namespace == "default"
                || namespace == "linux"
                || namespace.is_empty()
            {
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
            if namespace.starts_with("kube-")
                || namespace == "local-path-storage"
                || namespace == "default"
                || namespace == "linux"
                || namespace.is_empty()
            {
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
            if namespace.starts_with("kube-")
                || namespace == "local-path-storage"
                || namespace == "default"
                || namespace == "linux"
                || namespace.is_empty()
            {
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

    // Build nodes for all namespaces with traffic
    let mut nodes: Vec<NetworkNode> = Vec::new();
    for (i, (namespace, (rx, tx, pods))) in metrics_map.iter().enumerate() {
        nodes.push(NetworkNode {
            id: namespace.clone(),
            name: capitalize_first(namespace),
            node_type: "app".to_string(),
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
            rx_bytes_per_sec: total_tx,
            tx_bytes_per_sec: total_rx,
            total_traffic: total_rx + total_tx,
            pod_count: 0,
            color: "#6b7280".to_string(),
        });
    }

    // Discover edges from Kubernetes Services
    let mut edges: Vec<NetworkEdge> = Vec::new();
    let namespace_set: HashSet<String> = metrics_map.keys().cloned().collect();

    if let Some(k8s) = state.k8s_client.read().await.as_ref() {
        // Discover service connections by examining services and their endpoints
        edges = discover_service_connections(k8s, &namespace_set).await;
    }

    // Deduplicate edges
    edges.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));
    edges.dedup_by(|a, b| a.source == b.source && a.target == b.target);

    Ok(Json(NetworkTopology { nodes, edges }))
}

/// Discover service connections from Kubernetes
async fn discover_service_connections(
    k8s: &crate::services::K8sClient,
    namespaces: &HashSet<String>,
) -> Vec<NetworkEdge> {
    use k8s_openapi::api::core::v1::{ConfigMap, Endpoints, Service};
    use kube::api::{Api, ListParams};

    let mut edges: Vec<NetworkEdge> = Vec::new();
    let mut seen_edges: HashSet<(String, String)> = HashSet::new(); // Track source-target pairs
    let mut service_to_namespace: HashMap<String, String> = HashMap::new();

    // Build a map of service names to their namespaces
    for ns in namespaces {
        let services: Api<Service> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(svc_list) = services.list(&ListParams::default()).await {
            for svc in svc_list.items {
                if let Some(name) = &svc.metadata.name {
                    // Store both short name and FQDN
                    service_to_namespace.insert(name.clone(), ns.clone());
                    service_to_namespace.insert(format!("{}.{}", name, ns), ns.clone());
                    service_to_namespace
                        .insert(format!("{}.{}.svc.cluster.local", name, ns), ns.clone());
                }
            }
        }
    }

    // For each namespace, look for references to other services in ConfigMaps and environment
    for ns in namespaces {
        // Check ConfigMaps for service references
        let configmaps: Api<ConfigMap> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(cm_list) = configmaps.list(&ListParams::default()).await {
            for cm in cm_list.items {
                if let Some(data) = &cm.data {
                    for (_key, value) in data {
                        // Look for URLs or service references in config values
                        for (svc_name, target_ns) in &service_to_namespace {
                            if target_ns != ns && value.contains(svc_name) {
                                let edge_key = (ns.clone(), target_ns.clone());
                                if !seen_edges.contains(&edge_key) {
                                    seen_edges.insert(edge_key);
                                    edges.push(NetworkEdge {
                                        source: ns.clone(),
                                        target: target_ns.clone(),
                                        edge_type: "config".to_string(),
                                        port: None,
                                        protocol: Some("HTTP".to_string()),
                                        label: String::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check Endpoints to find cross-namespace connections
        let endpoints: Api<Endpoints> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(ep_list) = endpoints.list(&ListParams::default()).await {
            for ep in ep_list.items {
                if let Some(subsets) = &ep.subsets {
                    for subset in subsets {
                        if let Some(addresses) = &subset.addresses {
                            for addr in addresses {
                                // Check if this endpoint points to a pod in a different namespace
                                if let Some(target_ref) = &addr.target_ref {
                                    if let Some(target_ns) = &target_ref.namespace {
                                        if target_ns != ns && namespaces.contains(target_ns) {
                                            let edge_key = (ns.clone(), target_ns.clone());
                                            if !seen_edges.contains(&edge_key) {
                                                seen_edges.insert(edge_key);
                                                let port = subset
                                                    .ports
                                                    .as_ref()
                                                    .and_then(|p| p.first())
                                                    .map(|p| p.port);
                                                edges.push(NetworkEdge {
                                                    source: ns.clone(),
                                                    target: target_ns.clone(),
                                                    edge_type: "endpoint".to_string(),
                                                    port,
                                                    protocol: Some("TCP".to_string()),
                                                    label: String::new(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Look for common patterns in service names that indicate dependencies
    // e.g., nginx upstream configs, proxy configs, etc.
    for ns in namespaces {
        let services: Api<Service> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(svc_list) = services.list(&ListParams::default()).await {
            for svc in svc_list.items {
                if let Some(annotations) = svc.metadata.annotations {
                    // Check for upstream/backend annotations (common in ingress/proxy configs)
                    for (_key, value) in &annotations {
                        for (svc_name, target_ns) in &service_to_namespace {
                            if target_ns != ns && value.contains(svc_name) {
                                let edge_key = (ns.clone(), target_ns.clone());
                                if !seen_edges.contains(&edge_key) {
                                    seen_edges.insert(edge_key);
                                    let port = svc
                                        .spec
                                        .as_ref()
                                        .and_then(|s| s.ports.as_ref())
                                        .and_then(|p| p.first())
                                        .map(|p| p.port);
                                    edges.push(NetworkEdge {
                                        source: ns.clone(),
                                        target: target_ns.clone(),
                                        edge_type: "upstream".to_string(),
                                        port,
                                        protocol: Some("HTTP".to_string()),
                                        label: String::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Add external edge to namespaces that have LoadBalancer or NodePort services
    for ns in namespaces {
        let edge_key = ("external".to_string(), ns.clone());
        if seen_edges.contains(&edge_key) {
            continue;
        }
        let services: Api<Service> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(svc_list) = services.list(&ListParams::default()).await {
            for svc in svc_list.items {
                if let Some(spec) = &svc.spec {
                    if let Some(svc_type) = &spec.type_ {
                        if svc_type == "LoadBalancer" || svc_type == "NodePort" {
                            seen_edges.insert(edge_key.clone());
                            edges.push(NetworkEdge {
                                source: "external".to_string(),
                                target: ns.clone(),
                                edge_type: "ingress".to_string(),
                                port: spec.ports.as_ref().and_then(|p| p.first()).map(|p| p.port),
                                protocol: Some("HTTP".to_string()),
                                label: String::new(),
                            });
                            break; // One external edge per namespace is enough
                        }
                    }
                }
            }
        }
    }

    // Check for Ingress resources that indicate external access
    use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
    for ns in namespaces {
        let ingresses: Api<Ingress> = Api::namespaced(k8s.client().clone(), ns);
        if let Ok(ing_list) = ingresses.list(&ListParams::default()).await {
            for ing in ing_list.items {
                // Add external edge for namespaces with Ingress resources
                let ext_edge_key = ("external".to_string(), ns.clone());
                if !seen_edges.contains(&ext_edge_key) {
                    seen_edges.insert(ext_edge_key);
                    edges.push(NetworkEdge {
                        source: "external".to_string(),
                        target: ns.clone(),
                        edge_type: "ingress".to_string(),
                        port: Some(443),
                        protocol: Some("HTTPS".to_string()),
                        label: String::new(),
                    });
                }

                // Also check ingress backend services for connections
                if let Some(spec) = &ing.spec {
                    if let Some(rules) = &spec.rules {
                        for rule in rules {
                            if let Some(http) = &rule.http {
                                for path in &http.paths {
                                    if let Some(backend) = &path.backend.service {
                                        if let Some(target_ns) =
                                            service_to_namespace.get(&backend.name)
                                        {
                                            if target_ns != ns {
                                                let backend_edge_key = (ns.clone(), target_ns.clone());
                                                if !seen_edges.contains(&backend_edge_key) {
                                                    seen_edges.insert(backend_edge_key);
                                                    edges.push(NetworkEdge {
                                                        source: ns.clone(),
                                                        target: target_ns.clone(),
                                                        edge_type: "ingress-backend".to_string(),
                                                        port: backend.port.as_ref().and_then(|p| p.number),
                                                        protocol: Some("HTTP".to_string()),
                                                        label: String::new(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                break; // One external edge per namespace is enough
            }
        }
    }

    // Check NetworkPolicies to determine egress access to the internet
    // By default, Kubernetes allows all egress traffic. Only if there's a NetworkPolicy
    // that explicitly restricts egress would a namespace be blocked from the internet.
    for ns in namespaces {
        let netpols: Api<NetworkPolicy> = Api::namespaced(k8s.client().clone(), ns);
        let mut egress_blocked = false;

        if let Ok(np_list) = netpols.list(&ListParams::default()).await {
            for np in np_list.items {
                if let Some(spec) = &np.spec {
                    // Check if this policy has egress rules
                    if let Some(policy_types) = &spec.policy_types {
                        if policy_types.iter().any(|t| t == "Egress") {
                            // If there's an Egress policy type, check if it allows external
                            if let Some(egress_rules) = &spec.egress {
                                // If egress rules exist but are empty, all egress is blocked
                                if egress_rules.is_empty() {
                                    egress_blocked = true;
                                    break;
                                }
                                // If there are rules, check if they only allow cluster-internal
                                let allows_external = egress_rules.iter().any(|rule| {
                                    // Rule with no 'to' field allows all destinations
                                    if rule.to.is_none() {
                                        return true;
                                    }
                                    // Check if any 'to' allows external (0.0.0.0/0 or no ipBlock)
                                    if let Some(to_list) = &rule.to {
                                        to_list.iter().any(|peer| {
                                            if let Some(ip_block) = &peer.ip_block {
                                                // 0.0.0.0/0 allows all external
                                                ip_block.cidr == "0.0.0.0/0"
                                            } else {
                                                // No ipBlock means it could be namespace/pod selector
                                                // which doesn't restrict external access by itself
                                                peer.namespace_selector.is_none()
                                                    && peer.pod_selector.is_none()
                                            }
                                        })
                                    } else {
                                        true
                                    }
                                });
                                if !allows_external {
                                    egress_blocked = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If egress is not blocked, this namespace can reach the internet
        if !egress_blocked {
            let egress_edge_key = (ns.clone(), "external".to_string());
            if !seen_edges.contains(&egress_edge_key) {
                seen_edges.insert(egress_edge_key);
                edges.push(NetworkEdge {
                    source: ns.clone(),
                    target: "external".to_string(),
                    edge_type: "egress".to_string(),
                    port: None,
                    protocol: Some("TCP".to_string()),
                    label: String::new(),
                });
            }
        }
    }

    edges
}

/// Get detailed network statistics per app
async fn get_network_stats(
    State(_state): State<AppState>,
    Extension(_auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<NetworkStats>>> {
    // Query all network metrics - get ALL namespaces
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
                // Skip internal k8s namespaces
                if namespace.starts_with("kube-")
                    || namespace == "local-path-storage"
                    || namespace == "default"
                    || namespace == "linux"
                    || namespace.is_empty()
                {
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

/// Capitalize first letter
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
