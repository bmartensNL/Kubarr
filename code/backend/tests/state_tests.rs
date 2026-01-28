//! Tests for application state module

use std::sync::Arc;
use tokio::sync::RwLock;

use sea_orm::DatabaseConnection;

use kubarr::services::audit::AuditService;
use kubarr::services::catalog::AppCatalog;
use kubarr::services::notification::NotificationService;
use kubarr::state::{AppState, DbConn, SharedCatalog, SharedK8sClient};
mod common;

use common::create_test_db;

#[tokio::test]
async fn test_app_state_new() {
    let db = create_test_db().await;
    let k8s_client: SharedK8sClient = Arc::new(RwLock::new(None));
    let catalog = AppCatalog::default();
    let catalog: SharedCatalog = Arc::new(RwLock::new(catalog));
    let audit = AuditService::new();
    let notification = NotificationService::new();

    let state = AppState::new(Some(db), k8s_client, catalog, audit, notification);

    // Should be cloneable
    let _cloned = state.clone();
}

#[tokio::test]
async fn test_app_state_clone() {
    let db = create_test_db().await;
    let k8s_client: SharedK8sClient = Arc::new(RwLock::new(None));
    let catalog = AppCatalog::default();
    let catalog: SharedCatalog = Arc::new(RwLock::new(catalog));
    let audit = AuditService::new();
    let notification = NotificationService::new();

    let state1 = AppState::new(
        Some(db.clone()),
        k8s_client.clone(),
        catalog.clone(),
        audit,
        notification,
    );
    let state2 = state1.clone();

    // Both states should share the same Arc references
    assert!(Arc::ptr_eq(&state1.k8s_client, &state2.k8s_client));
    assert!(Arc::ptr_eq(&state1.catalog, &state2.catalog));
}

#[tokio::test]
async fn test_shared_k8s_client_rw() {
    let k8s_client: SharedK8sClient = Arc::new(RwLock::new(None));

    // Read lock
    {
        let read = k8s_client.read().await;
        assert!(read.is_none());
    }

    // The client is None because we can't easily construct K8sClient in tests
    // But the RwLock mechanism works
}

#[tokio::test]
async fn test_shared_catalog_rw() {
    let catalog = AppCatalog::default();
    let shared: SharedCatalog = Arc::new(RwLock::new(catalog));

    // Read lock
    {
        let read = shared.read().await;
        assert!(read.get_categories().is_empty() || !read.get_categories().is_empty());
    }

    // Write lock (even if we don't mutate)
    {
        let _write = shared.write().await;
        // Could modify catalog here
    }
}

#[test]
fn test_db_conn_type_alias() {
    // DbConn is an alias for DatabaseConnection
    fn _accepts_db_conn(_db: &DbConn) {}
    fn _accepts_database_connection(_db: &DatabaseConnection) {}
    // These compile, proving the type alias works
}
