use std::collections::HashMap;

use k8s_openapi::api::core::v1::{Pod, Secret, Service};
use kube::{
    api::{Api, ListParams, ObjectMeta},
    config::{Config, KubeConfigOptions, Kubeconfig},
    Client,
};
use serde::Deserialize;

use crate::config::CONFIG;
use crate::error::{AppError, Result};

/// Kubernetes client manager
pub struct K8sClient {
    client: Client,
}

impl K8sClient {
    /// Create a new Kubernetes client
    pub async fn new() -> Result<Self> {
        let client = if CONFIG.in_cluster {
            // In-cluster config
            let config = Config::incluster()?;
            Client::try_from(config)?
        } else if let Some(ref kubeconfig_path) = CONFIG.kubeconfig_path {
            // Explicit kubeconfig path
            let kubeconfig = Kubeconfig::read_from(kubeconfig_path)?;
            let config = Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default()).await?;
            Client::try_from(config)?
        } else {
            // Default kubeconfig
            Client::try_default().await?
        };

        Ok(Self { client })
    }

    /// Get the Kubernetes client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Test the Kubernetes connection
    pub async fn test_connection(&self) -> Result<bool> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), "default");
        pods.list(&ListParams::default().limit(1)).await?;
        Ok(true)
    }

    /// Get Kubernetes server version
    pub async fn get_server_version(&self) -> Result<String> {
        let version = self.client.apiserver_version().await?;
        Ok(format!("{}.{}", version.major, version.minor))
    }

    /// Check if metrics-server is available
    pub async fn check_metrics_server_available(&self) -> bool {
        // Try to list pod metrics
        let result = self
            .client
            .request::<PodMetricsList>(
                http::Request::get("/apis/metrics.k8s.io/v1beta1/pods?limit=1")
                    .body(vec![])
                    .unwrap(),
            )
            .await;

        result.is_ok()
    }

    /// Get pod status for a namespace
    pub async fn get_pod_status(
        &self,
        namespace: &str,
        app_name: Option<&str>,
    ) -> Result<Vec<PodStatus>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let lp = if let Some(app) = app_name {
            ListParams::default().labels(&format!("app.kubernetes.io/name={}", app))
        } else {
            ListParams::default()
        };

        let pod_list = pods.list(&lp).await?;

        let mut statuses = Vec::new();
        for pod in pod_list {
            let metadata = pod.metadata;
            let spec = pod.spec.unwrap_or_default();
            let status = pod.status.unwrap_or_default();

            let name = metadata.name.unwrap_or_default();
            let labels = metadata.labels.unwrap_or_default();

            // Calculate age
            let age = if let Some(creation) = metadata.creation_timestamp {
                let duration = chrono::Utc::now() - creation.0;
                format_age(duration)
            } else {
                "unknown".to_string()
            };

            // Get restart count
            let restart_count = status
                .container_statuses
                .as_ref()
                .map(|cs| cs.iter().map(|c| c.restart_count).sum())
                .unwrap_or(0);

            // Check if ready
            let ready = status
                .conditions
                .as_ref()
                .and_then(|conditions| {
                    conditions
                        .iter()
                        .find(|c| c.type_ == "Ready")
                        .map(|c| c.status == "True")
                })
                .unwrap_or(false);

            // Get app label
            let app_label = labels
                .get("app.kubernetes.io/name")
                .or_else(|| labels.get("app"))
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            statuses.push(PodStatus {
                name,
                app: app_label,
                namespace: namespace.to_string(),
                status: status.phase.unwrap_or_else(|| "Unknown".to_string()),
                ready,
                restart_count,
                age,
                node: spec.node_name,
                ip: status.pod_ip,
                cpu_usage: None,
                memory_usage: None,
            });
        }

        Ok(statuses)
    }

    /// Get pod metrics for a namespace
    pub async fn get_pod_metrics(
        &self,
        namespace: &str,
        app_name: Option<&str>,
    ) -> Result<Vec<PodMetrics>> {
        // Get pod metrics from metrics-server
        let url = format!("/apis/metrics.k8s.io/v1beta1/namespaces/{}/pods", namespace);
        let request = http::Request::get(&url).body(vec![]).unwrap();

        let metrics_list: PodMetricsList = match self.client.request(request).await {
            Ok(m) => m,
            Err(_) => return Ok(Vec::new()),
        };

        let mut metrics = Vec::new();
        for item in metrics_list.items {
            // Filter by app if specified
            if let Some(app) = app_name {
                let labels = item.metadata.labels.unwrap_or_default();
                let app_label = labels
                    .get("app.kubernetes.io/name")
                    .or_else(|| labels.get("app"));
                if app_label != Some(&app.to_string()) {
                    continue;
                }
            }

            let mut total_cpu = 0i64;
            let mut total_memory = 0i64;

            for container in &item.containers {
                total_cpu += parse_cpu(&container.usage.cpu);
                total_memory += parse_memory(&container.usage.memory);
            }

            metrics.push(PodMetrics {
                name: item.metadata.name.unwrap_or_default(),
                namespace: namespace.to_string(),
                cpu_usage: format_cpu(total_cpu),
                memory_usage: format_memory(total_memory),
            });
        }

        Ok(metrics)
    }

    /// Get service endpoints for an app
    pub async fn get_service_endpoints(
        &self,
        app_name: &str,
        namespace: &str,
    ) -> Result<Vec<ServiceEndpoint>> {
        let services: Api<Service> = Api::namespaced(self.client.clone(), namespace);

        let service = match services.get(app_name).await {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()),
        };

        let spec = service.spec.unwrap_or_default();
        let status = service.status.unwrap_or_default();

        let mut endpoints = Vec::new();
        for port in spec.ports.unwrap_or_default() {
            let port_num = port.port;
            let target_port = port.target_port.map(|tp| match tp {
                k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(i) => i.to_string(),
                k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(s) => s,
            });

            let port_forward_cmd = format!(
                "kubectl port-forward -n {} svc/{} {}:{}",
                namespace, app_name, port_num, port_num
            );

            // Check for external URL
            let external_url = if spec.type_.as_deref() == Some("LoadBalancer") {
                status
                    .load_balancer
                    .as_ref()
                    .and_then(|lb| lb.ingress.as_ref())
                    .and_then(|ingress| ingress.first())
                    .and_then(|ing| ing.ip.as_ref())
                    .map(|ip| format!("http://{}:{}", ip, port_num))
            } else {
                None
            };

            endpoints.push(ServiceEndpoint {
                name: app_name.to_string(),
                namespace: namespace.to_string(),
                port: port_num,
                target_port,
                port_forward_command: port_forward_cmd,
                url: external_url,
                service_type: spec.type_.clone().unwrap_or_else(|| "ClusterIP".to_string()),
            });
        }

        Ok(endpoints)
    }

    /// Sync OAuth2-proxy credentials to Kubernetes secret
    pub async fn sync_oauth2_proxy_secret(
        &self,
        client_id: &str,
        client_secret: &str,
        cookie_secret: &str,
        namespace: &str,
    ) -> Result<bool> {
        use base64::Engine;
        use k8s_openapi::ByteString;

        let secrets: Api<Secret> = Api::namespaced(self.client.clone(), namespace);
        let secret_name = "oauth2-proxy-credentials";

        let mut data = std::collections::BTreeMap::new();
        data.insert(
            "client-id".to_string(),
            ByteString(client_id.as_bytes().to_vec()),
        );
        data.insert(
            "client-secret".to_string(),
            ByteString(client_secret.as_bytes().to_vec()),
        );
        data.insert(
            "cookie-secret".to_string(),
            ByteString(cookie_secret.as_bytes().to_vec()),
        );

        let mut labels = std::collections::BTreeMap::new();
        labels.insert("app".to_string(), "oauth2-proxy".to_string());
        labels.insert("managed-by".to_string(), "kubarr".to_string());

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some(secret_name.to_string()),
                namespace: Some(namespace.to_string()),
                labels: Some(labels),
                ..Default::default()
            },
            data: Some(data),
            type_: Some("Opaque".to_string()),
            ..Default::default()
        };

        // Try to replace, if not exists create
        match secrets.replace(secret_name, &Default::default(), &secret).await {
            Ok(_) => Ok(true),
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                // Create new secret
                match secrets.create(&Default::default(), &secret).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }

    /// List pods in a namespace, optionally filtered by app name
    pub async fn list_pods(
        &self,
        namespace: &str,
        app_name: Option<&str>,
    ) -> Result<Vec<Pod>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let lp = if let Some(app) = app_name {
            ListParams::default().labels(&format!("app.kubernetes.io/name={}", app))
        } else {
            ListParams::default()
        };

        let pod_list = pods.list(&lp).await?;
        Ok(pod_list.items)
    }

    /// Get logs from a specific pod
    pub async fn get_pod_logs(
        &self,
        pod_name: &str,
        namespace: &str,
        container: Option<&str>,
        tail_lines: i32,
    ) -> Result<String> {
        use kube::api::LogParams;

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let mut log_params = LogParams {
            tail_lines: Some(tail_lines as i64),
            ..Default::default()
        };

        if let Some(c) = container {
            log_params.container = Some(c.to_string());
        }

        let logs = pods.logs(pod_name, &log_params).await?;
        Ok(logs)
    }
}

// ============================================================================
// Helper Types
// ============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct PodStatus {
    pub name: String,
    pub app: String,
    pub namespace: String,
    pub status: String,
    pub ready: bool,
    #[serde(rename = "restarts")]
    pub restart_count: i32,
    pub age: String,
    pub node: Option<String>,
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PodMetrics {
    pub name: String,
    pub namespace: String,
    pub cpu_usage: String,
    pub memory_usage: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceEndpoint {
    pub name: String,
    pub namespace: String,
    pub port: i32,
    pub target_port: Option<String>,
    pub port_forward_command: String,
    pub url: Option<String>,
    pub service_type: String,
}

// Metrics server response types
#[derive(Debug, Deserialize)]
struct PodMetricsList {
    items: Vec<PodMetricsItem>,
}

#[derive(Debug, Deserialize)]
struct PodMetricsItem {
    metadata: PodMetricsMetadata,
    containers: Vec<ContainerMetrics>,
}

#[derive(Debug, Deserialize)]
struct PodMetricsMetadata {
    name: Option<String>,
    labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct ContainerMetrics {
    usage: ContainerUsage,
}

#[derive(Debug, Deserialize)]
struct ContainerUsage {
    cpu: String,
    memory: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_age(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();

    if total_seconds < 60 {
        format!("{}s", total_seconds)
    } else if total_seconds < 3600 {
        format!("{}m", total_seconds / 60)
    } else if total_seconds < 86400 {
        format!("{}h", total_seconds / 3600)
    } else {
        format!("{}d", total_seconds / 86400)
    }
}

fn parse_cpu(cpu_str: &str) -> i64 {
    if cpu_str.ends_with('n') {
        cpu_str[..cpu_str.len() - 1].parse().unwrap_or(0)
    } else if cpu_str.ends_with('u') {
        cpu_str[..cpu_str.len() - 1].parse::<i64>().unwrap_or(0) * 1000
    } else if cpu_str.ends_with('m') {
        cpu_str[..cpu_str.len() - 1].parse::<i64>().unwrap_or(0) * 1_000_000
    } else {
        cpu_str.parse::<i64>().unwrap_or(0) * 1_000_000_000
    }
}

fn format_cpu(nanocores: i64) -> String {
    let millicores = nanocores / 1_000_000;
    if millicores < 1000 {
        format!("{}m", millicores)
    } else {
        format!("{:.2}", millicores as f64 / 1000.0)
    }
}

fn parse_memory(memory_str: &str) -> i64 {
    if memory_str.ends_with("Ki") {
        memory_str[..memory_str.len() - 2].parse::<i64>().unwrap_or(0) * 1024
    } else if memory_str.ends_with("Mi") {
        memory_str[..memory_str.len() - 2].parse::<i64>().unwrap_or(0) * 1024 * 1024
    } else if memory_str.ends_with("Gi") {
        memory_str[..memory_str.len() - 2].parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024
    } else if memory_str.ends_with("Ti") {
        memory_str[..memory_str.len() - 2].parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024 * 1024
    } else {
        memory_str.parse().unwrap_or(0)
    }
}

fn format_memory(bytes: i64) -> String {
    if bytes < 1024 * 1024 {
        format!("{}Ki", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{}Mi", bytes / (1024 * 1024))
    } else {
        format!("{:.2}Gi", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
