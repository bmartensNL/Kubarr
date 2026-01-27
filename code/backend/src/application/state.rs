use std::sync::Arc;
use tokio::sync::RwLock;

use sea_orm::DatabaseConnection;

use crate::services::audit::AuditService;
use crate::services::catalog::AppCatalog;
use crate::services::k8s::K8sClient;
use crate::services::notification::NotificationService;

/// Database connection type alias
pub type DbConn = DatabaseConnection;

/// Shared K8s client state
pub type SharedK8sClient = Arc<RwLock<Option<K8sClient>>>;

/// Shared app catalog state
pub type SharedCatalog = Arc<RwLock<AppCatalog>>;

/// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub db: DbConn,
    pub k8s_client: SharedK8sClient,
    pub catalog: SharedCatalog,
    pub audit: AuditService,
    pub notification: NotificationService,
}

impl AppState {
    pub fn new(
        db: DbConn,
        k8s_client: SharedK8sClient,
        catalog: SharedCatalog,
        audit: AuditService,
        notification: NotificationService,
    ) -> Self {
        Self {
            db,
            k8s_client,
            catalog,
            audit,
            notification,
        }
    }
}
