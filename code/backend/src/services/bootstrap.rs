use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};

use crate::db;
use crate::error::{AppError, Result};
use crate::models::prelude::*;
use crate::models::{bootstrap_status, server_config};
use crate::services::catalog::AppCatalog;
use crate::services::deployment::DeploymentManager;
use crate::services::k8s::K8sClient;
use crate::state::{AppState, SharedDbConn};

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
    #[serde(rename = "database_connected")]
    DatabaseConnected { message: String },
    #[serde(rename = "bootstrap_complete")]
    BootstrapComplete { message: String },
}

/// Bootstrap components to install in order
pub const BOOTSTRAP_COMPONENTS: &[(&str, &str)] = &[
    ("postgresql", "PostgreSQL"),
    ("victoriametrics", "VictoriaMetrics"),
    ("victorialogs", "VictoriaLogs"),
    ("fluent-bit", "Fluent Bit"),
];

/// In-memory status for components (used before database is available)
#[derive(Debug, Clone, Default)]
pub struct InMemoryStatus {
    pub statuses: Vec<ComponentStatus>,
    pub started: bool,
}

/// Bootstrap service for installing system components
pub struct BootstrapService {
    db: SharedDbConn,
    k8s: Arc<RwLock<Option<K8sClient>>>,
    catalog: Arc<RwLock<AppCatalog>>,
    pub broadcast_tx: broadcast::Sender<String>,
    /// In-memory status (used before PostgreSQL is installed)
    in_memory_status: Arc<RwLock<InMemoryStatus>>,
}

impl BootstrapService {
    pub fn new(
        db: SharedDbConn,
        k8s: Arc<RwLock<Option<K8sClient>>>,
        catalog: Arc<RwLock<AppCatalog>>,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Self {
        // Initialize in-memory status with all components
        let statuses = BOOTSTRAP_COMPONENTS
            .iter()
            .map(|(component, display_name)| ComponentStatus {
                component: component.to_string(),
                display_name: display_name.to_string(),
                status: "pending".to_string(),
                message: Some("Waiting to install".to_string()),
                error: None,
            })
            .collect();

        Self {
            db,
            k8s,
            catalog,
            broadcast_tx,
            in_memory_status: Arc::new(RwLock::new(InMemoryStatus {
                statuses,
                started: false,
            })),
        }
    }

    /// Get status of all bootstrap components
    pub async fn get_status(&self) -> Result<Vec<ComponentStatus>> {
        // Try database first
        let db_guard = self.db.read().await;
        if let Some(ref db) = *db_guard {
            let components = BootstrapStatus::find().all(db).await?;
            if !components.is_empty() {
                return Ok(components
                    .into_iter()
                    .map(|c| ComponentStatus {
                        component: c.component,
                        display_name: c.display_name,
                        status: c.status,
                        message: c.message,
                        error: c.error,
                    })
                    .collect());
            }
        }
        drop(db_guard);

        // Fall back to in-memory status
        let status = self.in_memory_status.read().await;
        Ok(status.statuses.clone())
    }

    /// Check if bootstrap is complete (all components healthy)
    pub async fn is_complete(&self) -> bool {
        match self.get_status().await {
            Ok(components) => {
                !components.is_empty() && components.iter().all(|c| c.status == "healthy")
            }
            Err(_) => false,
        }
    }

    /// Check if bootstrap has been started
    pub async fn has_started(&self) -> bool {
        // Check database first
        let db_guard = self.db.read().await;
        if let Some(ref db) = *db_guard {
            if let Ok(components) = BootstrapStatus::find().all(db).await {
                if components.iter().any(|c| c.status != "pending") {
                    return true;
                }
            }
        }
        drop(db_guard);

        // Check in-memory status
        let status = self.in_memory_status.read().await;
        status.started
    }

    /// Broadcast an event to all connected WebSocket clients
    fn broadcast(&self, event: BootstrapEvent) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.broadcast_tx.send(json);
        }
    }

    /// Update in-memory status for a component
    async fn update_in_memory_status(
        &self,
        component: &str,
        status: &str,
        message: Option<&str>,
        error: Option<&str>,
    ) {
        let mut mem_status = self.in_memory_status.write().await;
        if let Some(comp) = mem_status.statuses.iter_mut().find(|c| c.component == component) {
            comp.status = status.to_string();
            if let Some(msg) = message {
                comp.message = Some(msg.to_string());
            }
            if let Some(err) = error {
                comp.error = Some(err.to_string());
            }
        }
    }

    /// Update component status in database (if available)
    async fn update_db_status(
        &self,
        db: &DatabaseConnection,
        component: &str,
        status: &str,
        message: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let existing = BootstrapStatus::find()
            .filter(bootstrap_status::Column::Component.eq(component))
            .one(db)
            .await?;

        if let Some(existing) = existing {
            let mut active: bootstrap_status::ActiveModel = existing.into();
            active.status = Set(status.to_string());

            if let Some(msg) = message {
                active.message = Set(Some(msg.to_string()));
            }

            if let Some(err) = error {
                active.error = Set(Some(err.to_string()));
            }

            if status == "installing" {
                if let sea_orm::ActiveValue::Unchanged(None) = active.started_at {
                    active.started_at = Set(Some(Utc::now()));
                }
            }

            if status == "healthy" || status == "failed" {
                active.completed_at = Set(Some(Utc::now()));
            }

            active.update(db).await?;
        }
        Ok(())
    }

    /// Initialize bootstrap status records in database
    async fn initialize_db_status(&self, db: &DatabaseConnection) -> Result<()> {
        for (component, display_name) in BOOTSTRAP_COMPONENTS {
            let existing = BootstrapStatus::find()
                .filter(bootstrap_status::Column::Component.eq(*component))
                .one(db)
                .await?;

            if existing.is_none() {
                // Get current in-memory status
                let mem_status = self.in_memory_status.read().await;
                let current = mem_status.statuses.iter().find(|c| c.component == *component);

                let (status, message, error) = if let Some(c) = current {
                    (c.status.clone(), c.message.clone(), c.error.clone())
                } else {
                    ("pending".to_string(), Some("Waiting to install".to_string()), None)
                };

                let record = bootstrap_status::ActiveModel {
                    component: Set(component.to_string()),
                    display_name: Set(display_name.to_string()),
                    status: Set(status),
                    message: Set(message),
                    error: Set(error),
                    ..Default::default()
                };
                record.insert(db).await?;
            }
        }
        Ok(())
    }

    /// Check if PostgreSQL (CloudNativePG) is healthy
    async fn check_postgresql_health(&self) -> Result<bool> {
        let k8s_guard = self.k8s.read().await;
        let k8s = k8s_guard.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("Kubernetes client not available".to_string())
        })?;

        // Check if the kubarr-db-1 pod is running and ready
        match k8s.get_pod("kubarr", "kubarr-db-1").await {
            Ok(pod) => {
                if let Some(status) = pod.status {
                    if let Some(phase) = status.phase {
                        if phase == "Running" {
                            if let Some(conditions) = status.conditions {
                                for condition in conditions {
                                    if condition.type_ == "Ready" && condition.status == "True" {
                                        return Ok(true);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(false)
            }
            Err(_) => Ok(false),
        }
    }

    /// Install PostgreSQL using Helm chart
    async fn install_postgresql(&self) -> Result<()> {
        tracing::info!("Installing PostgreSQL via Helm chart");

        self.update_in_memory_status(
            "postgresql",
            "installing",
            Some("Deploying PostgreSQL cluster..."),
            None,
        )
        .await;
        self.broadcast(BootstrapEvent::ComponentStarted {
            component: "postgresql".to_string(),
            message: "Deploying PostgreSQL cluster...".to_string(),
        });

        // Deploy PostgreSQL using helm
        let result = tokio::process::Command::new("helm")
            .args([
                "upgrade",
                "--install",
                "postgresql",
                "/app/charts/postgresql",
                "-n",
                "kubarr",
                "--wait",
                "--timeout",
                "5m",
            ])
            .output()
            .await;

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
                    tracing::error!("PostgreSQL installation failed: {}", error_msg);
                    self.update_in_memory_status(
                        "postgresql",
                        "failed",
                        Some("Installation failed"),
                        Some(&error_msg),
                    )
                    .await;
                    self.broadcast(BootstrapEvent::ComponentFailed {
                        component: "postgresql".to_string(),
                        message: "PostgreSQL installation failed".to_string(),
                        error: error_msg.clone(),
                    });
                    return Err(AppError::Internal(error_msg));
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to run helm: {}", e);
                tracing::error!("{}", error_msg);
                self.update_in_memory_status(
                    "postgresql",
                    "failed",
                    Some("Installation failed"),
                    Some(&error_msg),
                )
                .await;
                self.broadcast(BootstrapEvent::ComponentFailed {
                    component: "postgresql".to_string(),
                    message: "PostgreSQL installation failed".to_string(),
                    error: error_msg.clone(),
                });
                return Err(AppError::Internal(error_msg));
            }
        }

        self.broadcast(BootstrapEvent::ComponentProgress {
            component: "postgresql".to_string(),
            message: "Waiting for PostgreSQL to become healthy...".to_string(),
            progress: 50,
        });

        // Wait for PostgreSQL to become healthy
        let max_attempts = 60;
        let mut attempts = 0;
        let mut healthy = false;

        while attempts < max_attempts {
            if self.check_postgresql_health().await.unwrap_or(false) {
                healthy = true;
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            attempts += 1;

            let progress = 50 + (attempts * 50 / max_attempts) as u8;
            self.broadcast(BootstrapEvent::ComponentProgress {
                component: "postgresql".to_string(),
                message: format!(
                    "Waiting for PostgreSQL to become healthy... ({}/{})",
                    attempts, max_attempts
                ),
                progress: progress.min(99),
            });
        }

        if !healthy {
            let error_msg = "PostgreSQL did not become healthy within timeout".to_string();
            self.update_in_memory_status(
                "postgresql",
                "failed",
                Some("Health check timeout"),
                Some(&error_msg),
            )
            .await;
            self.broadcast(BootstrapEvent::ComponentFailed {
                component: "postgresql".to_string(),
                message: "PostgreSQL health check failed".to_string(),
                error: error_msg.clone(),
            });
            return Err(AppError::Internal(error_msg));
        }

        self.update_in_memory_status(
            "postgresql",
            "healthy",
            Some("PostgreSQL is running"),
            None,
        )
        .await;
        self.broadcast(BootstrapEvent::ComponentCompleted {
            component: "postgresql".to_string(),
            message: "PostgreSQL installed successfully".to_string(),
        });

        Ok(())
    }

    /// Connect to database after PostgreSQL is installed
    async fn connect_to_database(&self) -> Result<()> {
        tracing::info!("Connecting to PostgreSQL database...");

        // Get database URL from K8s secret
        let k8s_guard = self.k8s.read().await;
        let k8s = k8s_guard.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("Kubernetes client not available".to_string())
        })?;

        let database_url = k8s.get_database_url("kubarr").await?;
        drop(k8s_guard);

        tracing::info!("Got database URL from K8s secret");

        // Connect to database
        let db = db::connect_with_url(&database_url).await?;
        tracing::info!("Database connection established");

        // Store connection in shared state
        {
            let mut db_guard = self.db.write().await;
            *db_guard = Some(db.clone());
        }

        // Initialize bootstrap status in database
        self.initialize_db_status(&db).await?;

        self.broadcast(BootstrapEvent::DatabaseConnected {
            message: "Database connection established".to_string(),
        });

        Ok(())
    }

    /// Install a single component using Helm
    async fn install_component(&self, component: &str, display_name: &str) -> Result<()> {
        // PostgreSQL is handled specially
        if component == "postgresql" {
            self.install_postgresql().await?;
            self.connect_to_database().await?;
            return Ok(());
        }

        tracing::info!("Installing bootstrap component: {}", component);

        // Get database connection (should be available after PostgreSQL is installed)
        let db_guard = self.db.read().await;
        let db = db_guard.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("Database not connected".to_string())
        })?;

        // Update status
        self.update_db_status(db, component, "installing", Some(&format!("Installing {}...", display_name)), None)
            .await?;
        self.update_in_memory_status(component, "installing", Some(&format!("Installing {}...", display_name)), None)
            .await;
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
                self.update_db_status(db, component, "failed", Some("Installation failed"), Some(&error_msg))
                    .await?;
                self.update_in_memory_status(component, "failed", Some("Installation failed"), Some(&error_msg))
                    .await;
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
        drop(db_guard);

        // Wait for deployment to become healthy
        let max_attempts = 60;
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

        // Get database again for final update
        let db_guard = self.db.read().await;
        let db = db_guard.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("Database not connected".to_string())
        })?;

        if healthy {
            self.update_db_status(db, component, "healthy", Some(&format!("{} is running", display_name)), None)
                .await?;
            self.update_in_memory_status(component, "healthy", Some(&format!("{} is running", display_name)), None)
                .await;
            self.broadcast(BootstrapEvent::ComponentCompleted {
                component: component.to_string(),
                message: format!("{} installed successfully", display_name),
            });
            Ok(())
        } else {
            let error_msg = format!("{} did not become healthy within timeout", display_name);
            self.update_db_status(db, component, "failed", Some("Health check timeout"), Some(&error_msg))
                .await?;
            self.update_in_memory_status(component, "failed", Some("Health check timeout"), Some(&error_msg))
                .await;
            self.broadcast(BootstrapEvent::ComponentFailed {
                component: component.to_string(),
                message: format!("{} health check failed", display_name),
                error: error_msg.clone(),
            });
            Err(AppError::Internal(error_msg))
        }
    }

    /// Start the bootstrap process
    pub async fn start_bootstrap(&self) -> Result<()> {
        // Mark as started
        {
            let mut mem_status = self.in_memory_status.write().await;
            mem_status.started = true;
        }

        // Install PostgreSQL first (sequential, not parallel)
        self.install_component("postgresql", "PostgreSQL").await?;

        // Now install remaining components in parallel
        let remaining_components: Vec<_> = BOOTSTRAP_COMPONENTS
            .iter()
            .filter(|(c, _)| *c != "postgresql")
            .collect();

        let mut handles = Vec::new();
        for (component, display_name) in remaining_components {
            let db = self.db.clone();
            let k8s = self.k8s.clone();
            let catalog = self.catalog.clone();
            let broadcast_tx = self.broadcast_tx.clone();
            let component = component.to_string();
            let display_name = display_name.to_string();

            let handle = tokio::spawn(async move {
                let service = BootstrapService::new(db, k8s, catalog, broadcast_tx);
                if let Err(e) = service.install_component(&component, &display_name).await {
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
        let comp = BOOTSTRAP_COMPONENTS
            .iter()
            .find(|(c, _)| *c == component)
            .ok_or_else(|| AppError::NotFound(format!("Unknown component: {}", component)))?;

        // Reset status
        self.update_in_memory_status(component, "pending", Some("Retrying installation..."), None)
            .await;

        // Also reset in database if available
        let db_guard = self.db.read().await;
        if let Some(ref db) = *db_guard {
            let _ = self.update_db_status(db, component, "pending", Some("Retrying installation..."), None).await;
        }
        drop(db_guard);

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

    let existing = ServerConfig::find().one(db).await?;

    if let Some(existing) = existing {
        let mut active: server_config::ActiveModel = existing.into();
        active.name = Set(name.to_string());
        active.storage_path = Set(storage_path.to_string());
        Ok(active.update(db).await?)
    } else {
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
