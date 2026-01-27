use std::collections::HashMap;
use std::process::Command;

use chrono::{DateTime, Utc};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Namespace;
use kube::api::{Api, DeleteParams, ListParams};
use serde::{Deserialize, Serialize};

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::services::catalog::AppCatalog;
use crate::services::K8sClient;

/// Deployment request
#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentRequest {
    pub app_name: String,
    #[serde(default)]
    pub custom_config: HashMap<String, String>,
}

/// Deployment status response
#[derive(Debug, Clone, Serialize)]
pub struct DeploymentStatus {
    pub app_name: String,
    pub namespace: String,
    pub status: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

/// Deployment manager for applications
pub struct DeploymentManager<'a> {
    k8s: &'a K8sClient,
    catalog: &'a AppCatalog,
}

impl<'a> DeploymentManager<'a> {
    pub fn new(k8s: &'a K8sClient, catalog: &'a AppCatalog) -> Self {
        Self { k8s, catalog }
    }

    /// Get the path to a Helm chart for an app
    fn get_chart_path(&self, app_name: &str) -> Option<std::path::PathBuf> {
        let chart_path = CONFIG.charts_dir.join(app_name);
        if chart_path.exists() && chart_path.join("Chart.yaml").exists() {
            Some(chart_path)
        } else {
            None
        }
    }

    /// Run a Helm command
    fn run_helm_command(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("helm")
            .args(args)
            .output()
            .map_err(|e| AppError::Internal(format!("Failed to run helm: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Internal(format!(
                "Helm command failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Deploy an application using Helm
    pub async fn deploy_app(
        &self,
        request: &DeploymentRequest,
        storage_path: Option<&str>,
    ) -> Result<DeploymentStatus> {
        // Get app config from catalog
        let app_config = self.catalog.get_app(&request.app_name).ok_or_else(|| {
            AppError::NotFound(format!("App '{}' not found in catalog", request.app_name))
        })?;

        // Check if Helm chart exists
        let chart_path = self.get_chart_path(&request.app_name).ok_or_else(|| {
            AppError::NotFound(format!(
                "No Helm chart found for app '{}'",
                request.app_name
            ))
        })?;

        let namespace = &request.app_name;

        // Build helm upgrade --install command
        let mut helm_args = vec![
            "upgrade",
            "--install",
            &request.app_name,
            chart_path.to_str().unwrap(),
            "-n",
            namespace,
            "--create-namespace",
        ];

        // Collect --set arguments
        let mut set_args: Vec<String> = Vec::new();

        // Add storage configuration
        if let Some(path) = storage_path {
            set_args.push(format!("storage.hostPath.enabled=true"));
            set_args.push(format!("storage.hostPath.rootPath={}", path));
        }

        // Add custom config
        for (key, value) in &request.custom_config {
            set_args.push(format!("{}={}", key, value));
        }

        // Add --set arguments
        for arg in &set_args {
            helm_args.push("--set");
            helm_args.push(arg);
        }

        // Run helm command
        let args_str: Vec<&str> = helm_args.iter().map(|s| s.as_ref()).collect();
        self.run_helm_command(&args_str)?;

        Ok(DeploymentStatus {
            app_name: request.app_name.clone(),
            namespace: namespace.to_string(),
            status: "installing".to_string(),
            message: format!("Deploying {}", app_config.display_name),
            timestamp: Utc::now(),
        })
    }

    /// Remove an application
    pub async fn remove_app(&self, app_name: &str) -> Result<bool> {
        let namespace = app_name;

        // Try to uninstall with Helm
        let _ = self.run_helm_command(&["uninstall", app_name, "-n", namespace]);

        // Delete the namespace
        let namespaces: Api<Namespace> = Api::all(self.k8s.client().clone());

        match namespaces.delete(namespace, &DeleteParams::default()).await {
            Ok(_) => Ok(true),
            Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(true),
            Err(e) => Err(AppError::Internal(format!(
                "Failed to delete namespace: {}",
                e
            ))),
        }
    }

    /// Get list of deployed app names
    pub async fn get_deployed_apps(&self) -> Vec<String> {
        let namespaces: Api<Namespace> = Api::all(self.k8s.client().clone());

        // Get all catalog app names
        let catalog_apps: std::collections::HashSet<_> = self
            .catalog
            .get_all_apps()
            .iter()
            .map(|app| app.name.clone())
            .collect();

        let mut deployed_apps = Vec::new();

        if let Ok(ns_list) = namespaces.list(&ListParams::default()).await {
            for ns in ns_list {
                if let Some(name) = ns.metadata.name {
                    if catalog_apps.contains(&name) {
                        // Check if there are deployments in this namespace
                        if let Ok(health) = self.check_namespace_health(&name).await {
                            if health.get("deployments").is_some() {
                                deployed_apps.push(name);
                            }
                        }
                    }
                }
            }
        }

        deployed_apps
    }

    /// Check if a namespace exists
    pub async fn check_namespace_exists(&self, namespace: &str) -> bool {
        let namespaces: Api<Namespace> = Api::all(self.k8s.client().clone());
        namespaces.get(namespace).await.is_ok()
    }

    /// Check if all deployments in a namespace are healthy
    pub async fn check_namespace_health(&self, namespace: &str) -> Result<serde_json::Value> {
        let namespaces: Api<Namespace> = Api::all(self.k8s.client().clone());

        // Check if namespace exists
        if namespaces.get(namespace).await.is_err() {
            return Ok(serde_json::json!({
                "status": "not_found",
                "healthy": false,
                "message": "Namespace does not exist"
            }));
        }

        // Get deployments
        let deployments: Api<Deployment> = Api::namespaced(self.k8s.client().clone(), namespace);
        let deploy_list = deployments.list(&ListParams::default()).await?;

        if deploy_list.items.is_empty() {
            return Ok(serde_json::json!({
                "status": "no_deployments",
                "healthy": false,
                "message": "No deployments found in namespace"
            }));
        }

        let mut all_healthy = true;
        let mut deployment_statuses = Vec::new();

        for deploy in &deploy_list.items {
            let name = deploy.metadata.name.clone().unwrap_or_default();
            let spec = deploy.spec.as_ref();
            let status = deploy.status.as_ref();

            let replicas = spec.and_then(|s| s.replicas).unwrap_or(1);
            let ready_replicas = status.and_then(|s| s.ready_replicas).unwrap_or(0);
            let available_replicas = status.and_then(|s| s.available_replicas).unwrap_or(0);

            let is_healthy = ready_replicas >= replicas && available_replicas >= replicas;

            deployment_statuses.push(serde_json::json!({
                "name": name,
                "replicas": replicas,
                "ready_replicas": ready_replicas,
                "available_replicas": available_replicas,
                "healthy": is_healthy
            }));

            if !is_healthy {
                all_healthy = false;
            }
        }

        Ok(serde_json::json!({
            "status": if all_healthy { "healthy" } else { "unhealthy" },
            "healthy": all_healthy,
            "deployments": deployment_statuses,
            "message": if all_healthy { "All deployments healthy" } else { "Some deployments are not healthy" }
        }))
    }
}
