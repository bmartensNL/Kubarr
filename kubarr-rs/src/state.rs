use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::DbPool;
use crate::services::catalog::AppCatalog;
use crate::services::k8s::K8sClient;

/// Shared K8s client state
pub type SharedK8sClient = Arc<RwLock<Option<K8sClient>>>;

/// Shared app catalog state
pub type SharedCatalog = Arc<RwLock<AppCatalog>>;

/// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub k8s_client: SharedK8sClient,
    pub catalog: SharedCatalog,
}

impl AppState {
    pub fn new(pool: DbPool, k8s_client: SharedK8sClient, catalog: SharedCatalog) -> Self {
        Self {
            pool,
            k8s_client,
            catalog,
        }
    }
}
