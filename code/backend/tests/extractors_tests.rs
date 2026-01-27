//! Tests for API extractors (authentication and authorization)

use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use kubarr::api::extractors::{get_user_app_access, get_user_permissions, user_has_app_access, user_has_permission};
use kubarr::models::prelude::*;
use kubarr::models::{role, user_role};
use kubarr::test_helpers::{create_test_db_with_seed, create_test_user, create_test_user_with_role};

// ==========================================================================
// Permission Checking Tests
// ==========================================================================

#[tokio::test]
async fn test_admin_has_all_permissions() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "admin_user", "admin@test.com", "password", "admin").await;

    // Admin should have all permissions
    assert!(user_has_permission(&db, user.id, "apps.view").await);
    assert!(user_has_permission(&db, user.id, "apps.install").await);
    assert!(user_has_permission(&db, user.id, "users.manage").await);
    assert!(user_has_permission(&db, user.id, "settings.manage").await);
    assert!(user_has_permission(&db, user.id, "any.permission").await); // Admin bypasses checks
}

#[tokio::test]
async fn test_viewer_has_limited_permissions() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "viewer_user", "viewer@test.com", "password", "viewer")
            .await;

    // Viewer should have view permissions
    assert!(user_has_permission(&db, user.id, "apps.view").await);
    assert!(user_has_permission(&db, user.id, "logs.view").await);
    assert!(user_has_permission(&db, user.id, "monitoring.view").await);

    // Viewer should NOT have management permissions
    assert!(!user_has_permission(&db, user.id, "apps.install").await);
    assert!(!user_has_permission(&db, user.id, "users.manage").await);
    assert!(!user_has_permission(&db, user.id, "settings.manage").await);
}

#[tokio::test]
async fn test_downloader_has_download_permissions() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "dl_user", "dl@test.com", "password", "downloader").await;

    // Downloader should have specific permissions
    assert!(user_has_permission(&db, user.id, "apps.view").await);
    assert!(user_has_permission(&db, user.id, "apps.restart").await);
    assert!(user_has_permission(&db, user.id, "storage.view").await);

    // Downloader should NOT have admin permissions
    assert!(!user_has_permission(&db, user.id, "apps.install").await);
    assert!(!user_has_permission(&db, user.id, "users.manage").await);
}

#[tokio::test]
async fn test_user_without_role_has_no_permissions() {
    let db = create_test_db_with_seed().await;
    let user = create_test_user(&db, "norole_user", "norole@test.com", "password", true).await;

    // User without roles should have no permissions
    assert!(!user_has_permission(&db, user.id, "apps.view").await);
    assert!(!user_has_permission(&db, user.id, "logs.view").await);
    assert!(!user_has_permission(&db, user.id, "anything").await);
}

// ==========================================================================
// App Access Tests
// ==========================================================================

#[tokio::test]
async fn test_admin_has_all_app_access() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "admin_user", "admin@test.com", "password", "admin").await;

    // Admin should have wildcard access
    let apps = get_user_app_access(&db, user.id).await;
    assert_eq!(apps, vec!["*"]);

    // Admin should have access to any app
    assert!(user_has_app_access(&db, user.id, "sonarr").await);
    assert!(user_has_app_access(&db, user.id, "qbittorrent").await);
    assert!(user_has_app_access(&db, user.id, "anyapp").await);
}

#[tokio::test]
async fn test_viewer_has_limited_app_access() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "viewer_user", "viewer@test.com", "password", "viewer")
            .await;

    // Viewer should have access to jellyfin and jellyseerr (as seeded)
    assert!(user_has_app_access(&db, user.id, "jellyfin").await);
    assert!(user_has_app_access(&db, user.id, "jellyseerr").await);

    // Viewer should NOT have access to download apps
    assert!(!user_has_app_access(&db, user.id, "qbittorrent").await);
    assert!(!user_has_app_access(&db, user.id, "transmission").await);
}

#[tokio::test]
async fn test_downloader_has_download_app_access() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "dl_user", "dl@test.com", "password", "downloader").await;

    // Downloader should have access to download clients
    assert!(user_has_app_access(&db, user.id, "qbittorrent").await);
    assert!(user_has_app_access(&db, user.id, "transmission").await);
    assert!(user_has_app_access(&db, user.id, "deluge").await);

    // Downloader should NOT have access to media apps (unless explicitly granted)
    assert!(!user_has_app_access(&db, user.id, "jellyfin").await);
}

#[tokio::test]
async fn test_user_without_role_has_no_app_access() {
    let db = create_test_db_with_seed().await;
    let user = create_test_user(&db, "norole_user", "norole@test.com", "password", true).await;

    // User without roles should have no app access
    let apps = get_user_app_access(&db, user.id).await;
    assert!(apps.is_empty());

    assert!(!user_has_app_access(&db, user.id, "sonarr").await);
    assert!(!user_has_app_access(&db, user.id, "jellyfin").await);
}

// ==========================================================================
// Get User Permissions Tests
// ==========================================================================

#[tokio::test]
async fn test_get_admin_permissions() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "admin_user", "admin@test.com", "password", "admin").await;

    let permissions = get_user_permissions(&db, user.id).await;

    // Admin should have all permissions including app.* wildcard
    assert!(permissions.contains(&"apps.view".to_string()));
    assert!(permissions.contains(&"apps.install".to_string()));
    assert!(permissions.contains(&"users.manage".to_string()));
    assert!(permissions.contains(&"app.*".to_string()));
}

#[tokio::test]
async fn test_get_viewer_permissions() {
    let db = create_test_db_with_seed().await;
    let user =
        create_test_user_with_role(&db, "viewer_user", "viewer@test.com", "password", "viewer")
            .await;

    let permissions = get_user_permissions(&db, user.id).await;

    // Viewer should have view permissions and app-specific permissions
    assert!(permissions.contains(&"apps.view".to_string()));
    assert!(permissions.contains(&"logs.view".to_string()));
    assert!(permissions.contains(&"app.jellyfin".to_string()));
    assert!(permissions.contains(&"app.jellyseerr".to_string()));

    // Viewer should NOT have management permissions
    assert!(!permissions.contains(&"apps.install".to_string()));
    assert!(!permissions.contains(&"users.manage".to_string()));
}

#[tokio::test]
async fn test_get_permissions_empty_for_no_role() {
    let db = create_test_db_with_seed().await;
    let user = create_test_user(&db, "norole_user", "norole@test.com", "password", true).await;

    let permissions = get_user_permissions(&db, user.id).await;
    assert!(permissions.is_empty());
}

#[tokio::test]
async fn test_permissions_are_deduplicated() {
    let db = create_test_db_with_seed().await;

    // Create a user with multiple roles (viewer + downloader)
    let user = create_test_user(&db, "multi_user", "multi@test.com", "password", true).await;

    // Assign both viewer and downloader roles
    for role_name in ["viewer", "downloader"] {
        let found_role = Role::find()
            .filter(role::Column::Name.eq(role_name))
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let ur = user_role::ActiveModel {
            user_id: Set(user.id),
            role_id: Set(found_role.id),
        };
        ur.insert(&db).await.unwrap();
    }

    let permissions = get_user_permissions(&db, user.id).await;

    // Both roles have apps.view, but it should only appear once
    let apps_view_count = permissions.iter().filter(|p| *p == "apps.view").count();
    assert_eq!(apps_view_count, 1, "apps.view should only appear once");

    // Permissions should be sorted
    let mut sorted = permissions.clone();
    sorted.sort();
    assert_eq!(permissions, sorted, "Permissions should be sorted");
}
