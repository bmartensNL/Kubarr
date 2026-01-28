use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};

use crate::error::{AppError, Result};
use crate::models::prelude::*;
use crate::models::{bootstrap_status, server_config};
use crate::services::catalog::AppCatalog;
use crate::services::deployment::DeploymentManager;
use crate::services::k8s::K8sClient;

/// Component status for bootstrap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub component: String,
    pub display_name: String,
    pub status: String, // 'pending', 'installing', 'healthy', 'failed'
    pub message: Option<String>,
    pub error: Option<String>,
}

/// Bootstrap progress event sent via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BootstrapEvent {
    #[serde(rename = "component_started")]
    ComponentStarted { component: String, message: String },
    #[serde(rename = "component_progress")]
    ComponentProgress {
        component: String,
        message: String,
        progress: u8,
    },
    #[serde(rename = "component_completed")]
    ComponentCompleted { component: String, message: String },
    #[serde(rename = "component_failed")]
    ComponentFailed {
        component: String,
        message: String,
        error: String,
    },
    #[serde(rename = "bootstrap_complete")]
    BootstrapComplete { message: String },
}

/// Bootstrap components to install in order
pub const BOOTSTRAP_COMPONENTS: &[(&str, &str)] = &[
    ("victoriametrics", "VictoriaMetrics"),
    ("victorialogs", "VictoriaLogs"),
    ("fluent-bit", "Fluent Bit"),
];

/// Bootstrap service for installing system components
pub struct BootstrapService {
    db: DatabaseConnection,
    k8s: Arc<RwLock<Option<K8sClient>>>,
    catalog: Arc<RwLock<AppCatalog>>,
    pub broadcast_tx: broadcast::Sender<String>,
}

impl BootstrapService {
    pub fn new(
        db: DatabaseConnection,
        k8s: Arc<RwLock<Option<K8sClient>>>,
        catalog: Arc<RwLock<AppCatalog>>,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Self {
        Self {
            db,
            k8s,
            catalog,
            broadcast_tx,
        }
    }

    /// Initialize bootstrap status records if they don't exist
    pub async fn initialize_status(&self) -> Result<()> {
        for (component, display_name) in BOOTSTRAP_COMPONENTS {
            let existing = BootstrapStatus::find()
                .filter(bootstrap_status::Column::Component.eq(*component))
                .one(&self.db)
                .await?;

            if existing.is_none() {
                let status = bootstrap_status::ActiveModel {
                    component: Set(component.to_string()),
                    display_name: Set(display_name.to_string()),
                    status: Set("pending".to_string()),
                    message: Set(Some("Waiting to install".to_string())),
                    ..Default::default()
                };
                status.insert(&self.db).await?;
            }
        }
        Ok(())
    }

    /// Get status of all bootstrap components
    pub async fn get_status(&self) -> Result<Vec<ComponentStatus>> {
        let components = BootstrapStatus::find().all(&self.db).await?;

        Ok(components
            .into_iter()
            .map(|c| ComponentStatus {
                component: c.component,
                display_name: c.display_name,
                status: c.status,
                message: c.message,
                error: c.error,
            })
            .collect())
    }

    /// Check if bootstrap is complete (all components healthy)
    pub async fn is_complete(&self) -> bool {
        let components = match BootstrapStatus::find().all(&self.db).await {
            Ok(c) => c,
            Err(_) => return false,
        };

        if components.is_empty() {
            return false;
        }

        components.iter().all(|c| c.status == "healthy")
    }

    /// Check if bootstrap has been started
    pub async fn has_started(&self) -> bool {
        let components = match BootstrapStatus::find().all(&self.db).await {
            Ok(c) => c,
            Err(_) => return false,
        };

        components
            .iter()
            .any(|c| c.status != "pending" || c.started_at.is_some())
    }

    /// Broadcast an event to all connected WebSocket clients
    fn broadcast(&self, event: BootstrapEvent) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.broadcast_tx.send(json);
        }
    }

    /// Update component status in database
    async fn update_status(
        &self,
        component: &str,
        status: &str,
        message: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let existing = BootstrapStatus::find()
            .filter(bootstrap_status::Column::Component.eq(component))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Component {} not found", component)))?;

        let mut active: bootstrap_status::ActiveModel = existing.into();
        active.status = Set(status.to_string());

        if let Some(msg) = message {
            active.message = Set(Some(msg.to_string()));
        }

        if let Some(err) = error {
            active.error = Set(Some(err.to_string()));
        }

        if status == "installing" {
            // Only set started_at if it hasn't been set yet
            if let sea_orm::ActiveValue::Unchanged(None) = active.started_at {
                active.started_at = Set(Some(Utc::now()));
            }
        }

        if status == "healthy" || status == "failed" {
            active.completed_at = Set(Some(Utc::now()));
        }

        active.update(&self.db).await?;
        Ok(())
    }

    /// Install a single component using Helm
    async fn install_component(&self, component: &str, display_name: &str) -> Result<()> {
        tracing::info!("Installing bootstrap component: {}", component);

        // Update status to installing
        self.update_status(component, "installing", Some(&format!("Installing {}...", display_name)), None)
            .await?;
        self.broadcast(BootstrapEvent::ComponentStarted {
            component: component.to_string(),
            message: format!("Installing {}...", display_name),
        });

        // Get K8s client and catalog
        let k8s_guard = self.k8s.read().await;
        let k8s = k8s_guard.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("Kubernetes client not available".to_string())
        })?;

        let catalog_guard = self.catalog.read().await;

        // Create deployment manager
        let deployment_manager = DeploymentManager::new(k8s, &catalog_guard);

        // Deploy the component
        let request = crate::services::deployment::DeploymentRequest {
            app_name: component.to_string(),
            custom_config: std::collections::HashMap::new(),
        };

        match deployment_manager.deploy_app(&request, None).await {
            Ok(_status) => {
                self.broadcast(BootstrapEvent::ComponentProgress {
                    component: component.to_string(),
                    message: format!("Deployed {}, waiting for health check...", display_name),
                    progress: 50,
                });
            }
            Err(e) => {
                let error_msg = format!("Failed to deploy {}: {}", display_name, e);
                tracing::error!("{}", error_msg);
                self.update_status(component, "failed", Some("Installation failed"), Some(&error_msg))
                    .await?;
                self.broadcast(BootstrapEvent::ComponentFailed {
                    component: component.to_string(),
                    message: format!("{} installation failed", display_name),
                    error: error_msg.clone(),
                });
                return Err(AppError::Internal(error_msg));
            }
        }

        // Drop the guards before the health check loop
        drop(catalog_guard);
        drop(k8s_guard);

        // Wait for deployment to become healthy (with timeout)
        let max_attempts = 60; // 5 minutes with 5-second intervals
        let mut attempts = 0;
        let mut healthy = false;

        while attempts < max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            attempts += 1;

            let k8s_guard = self.k8s.read().await;
            if let Some(k8s) = k8s_guard.as_ref() {
                let catalog_guard = self.catalog.read().await;
                let dm = DeploymentManager::new(k8s, &catalog_guard);

                if let Ok(health) = dm.check_namespace_health(component).await {
                    if health.get("healthy").and_then(|v| v.as_bool()).unwrap_or(false) {
                        healthy = true;
                        break;
                    }
                }
            }

            // Broadcast progress
            let progress = 50 + (attempts * 50 / max_attempts) as u8;
            self.broadcast(BootstrapEvent::ComponentProgress {
                component: component.to_string(),
                message: format!(
                    "Waiting for {} to become healthy... ({}/{})",
                    display_name, attempts, max_attempts
                ),
                progress: progress.min(99),
            });
        }

        if healthy {
            self.update_status(component, "healthy", Some(&format!("{} is running", display_name)), None)
                .await?;
            self.broadcast(BootstrapEvent::ComponentCompleted {
                component: component.to_string(),
                message: format!("{} installed successfully", display_name),
            });
            Ok(())
        } else {
            let error_msg = format!("{} did not become healthy within timeout", display_name);
            self.update_status(component, "failed", Some("Health check timeout"), Some(&error_msg))
                .await?;
            self.broadcast(BootstrapEvent::ComponentFailed {
                component: component.to_string(),
                message: format!("{} health check failed", display_name),
                error: error_msg.clone(),
            });
            Err(AppError::Internal(error_msg))
        }
    }

    /// Start the bootstrap process (installs all components in parallel)
    pub async fn start_bootstrap(&self) -> Result<()> {
        // Initialize status records
        self.initialize_status().await?;

        // Collect components that need to be installed
        let mut components_to_install = Vec::new();
        for (component, display_name) in BOOTSTRAP_COMPONENTS {
            let existing = BootstrapStatus::find()
                .filter(bootstrap_status::Column::Component.eq(*component))
                .one(&self.db)
                .await?;

            if let Some(ref status) = existing {
                if status.status == "healthy" {
                    tracing::info!("Component {} already healthy, skipping", component);
                    continue;
                }
            }
            components_to_install.push((*component, *display_name));
        }

        // Install all components in parallel
        let mut handles = Vec::new();
        for (component, display_name) in components_to_install {
            let db = self.db.clone();
            let k8s = self.k8s.clone();
            let catalog = self.catalog.clone();
            let broadcast_tx = self.broadcast_tx.clone();

            let handle = tokio::spawn(async move {
                let service = BootstrapService::new(db, k8s, catalog, broadcast_tx);
                if let Err(e) = service.install_component(component, display_name).await {
                    tracing::error!("Failed to install {}: {}", component, e);
                }
            });
            handles.push(handle);
        }

        // Wait for all installations to complete
        for handle in handles {
            let _ = handle.await;
        }

        // Check if all components are healthy
        if self.is_complete().await {
            self.broadcast(BootstrapEvent::BootstrapComplete {
                message: "All system components installed successfully".to_string(),
            });
        }

        Ok(())
    }

    /// Retry installing a failed component
    pub async fn retry_component(&self, component: &str) -> Result<()> {
        // Find the component
        let comp = BOOTSTRAP_COMPONENTS
            .iter()
            .find(|(c, _)| *c == component)
            .ok_or_else(|| AppError::NotFound(format!("Unknown component: {}", component)))?;

        // Reset status to pending
        self.update_status(component, "pending", Some("Retrying installation..."), None)
            .await?;

        // Install the component
        self.install_component(comp.0, comp.1).await
    }
}

/// Save server configuration
pub async fn save_server_config(
    db: &DatabaseConnection,
    name: &str,
    storage_path: &str,
) -> Result<server_config::Model> {
    let now = Utc::now();

    // Check if config already exists
    let existing = ServerConfig::find().one(db).await?;

    if let Some(existing) = existing {
        // Update existing config
        let mut active: server_config::ActiveModel = existing.into();
        active.name = Set(name.to_string());
        active.storage_path = Set(storage_path.to_string());
        Ok(active.update(db).await?)
    } else {
        // Create new config
        let config = server_config::ActiveModel {
            name: Set(name.to_string()),
            storage_path: Set(storage_path.to_string()),
            created_at: Set(now),
            ..Default::default()
        };
        Ok(config.insert(db).await?)
    }
}

/// Get server configuration
pub async fn get_server_config(db: &DatabaseConnection) -> Result<Option<server_config::Model>> {
    Ok(ServerConfig::find().one(db).await?)
}
