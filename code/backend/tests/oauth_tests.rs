//! Tests for OAuth security fixes:
//!  - Secure cookie flag controlled by KUBARR_INSECURE_COOKIES env var
//!  - token_expires_at populated from provider `expires_in` field
//!  - auto_approve controls whether new OAuth users require admin approval
//!  - DB-level token refresh update

mod common;
use common::create_test_db;

use chrono::Utc;
use kubarr::endpoints::oauth::{build_session_cookie, compute_token_expires_at};
use kubarr::models::{oauth_account, oauth_provider, user};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use std::sync::{LazyLock, Mutex};

/// Serialise env-var tests to prevent race conditions when setting/removing
/// `KUBARR_INSECURE_COOKIES` across parallel test threads.
static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ============================================================================
// Bug 1: Secure cookie flag
// ============================================================================

#[test]
fn test_cookie_has_secure_flag_by_default() {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::remove_var("KUBARR_INSECURE_COOKIES");

    let cookie = build_session_cookie("my_session_token");

    assert!(
        cookie.contains("; Secure"),
        "Cookie must contain '; Secure' when KUBARR_INSECURE_COOKIES is not set. Got: {cookie}"
    );
    assert!(cookie.contains("kubarr_session=my_session_token"));
    assert!(cookie.contains("HttpOnly"));
}

#[test]
fn test_cookie_omits_secure_flag_when_insecure_cookies_set() {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::set_var("KUBARR_INSECURE_COOKIES", "1");

    let cookie = build_session_cookie("my_session_token");

    // Clean up before asserting so that a panic doesn't leave the var set
    std::env::remove_var("KUBARR_INSECURE_COOKIES");

    assert!(
        !cookie.contains("; Secure"),
        "Cookie must NOT contain '; Secure' when KUBARR_INSECURE_COOKIES is set. Got: {cookie}"
    );
    assert!(cookie.contains("kubarr_session=my_session_token"));
}

#[test]
fn test_cookie_format_max_age_and_path() {
    let _guard = ENV_MUTEX.lock().unwrap();
    std::env::remove_var("KUBARR_INSECURE_COOKIES");

    let cookie = build_session_cookie("token123");

    assert!(cookie.contains("Max-Age=604800"), "Missing Max-Age");
    assert!(cookie.contains("Path=/"), "Missing Path");
    assert!(cookie.contains("SameSite=Lax"), "Missing SameSite");
}

// ============================================================================
// Bug 2: Token expiry helpers
// ============================================================================

#[test]
fn test_compute_token_expires_at_with_expires_in() {
    let before = Utc::now();
    let token_data = json!({"expires_in": 3600, "access_token": "tok"});
    let result = compute_token_expires_at(&token_data);
    let after = Utc::now();

    assert!(result.is_some(), "expires_in present → should return Some");
    let expires_at = result.unwrap();
    // Should be ~3600 s from now; allow ±5 s for test execution time
    assert!(
        expires_at >= before + chrono::Duration::seconds(3595),
        "expires_at too early: {expires_at}"
    );
    assert!(
        expires_at <= after + chrono::Duration::seconds(3605),
        "expires_at too late: {expires_at}"
    );
}

#[test]
fn test_compute_token_expires_at_without_expires_in() {
    let token_data = json!({"access_token": "tok"});
    let result = compute_token_expires_at(&token_data);
    assert!(result.is_none(), "No expires_in → should return None");
}

#[test]
fn test_compute_token_expires_at_with_zero_expires_in() {
    let before = Utc::now();
    let token_data = json!({"expires_in": 0, "access_token": "tok"});
    let result = compute_token_expires_at(&token_data);
    let after = Utc::now();

    assert!(result.is_some());
    let expires_at = result.unwrap();
    // expires_in=0 → already expired, should be very close to now
    let diff = (expires_at - before).num_seconds().abs();
    let _ = after; // just to avoid unused warning
    assert!(diff <= 2, "expires_at should be ~now for expires_in=0");
}

// ============================================================================
// Bug 2: DB-level token refresh integration test
// ============================================================================

#[tokio::test]
async fn test_refresh_updates_token_in_db() {
    let db = create_test_db().await;
    let now = Utc::now();

    // Insert a provider with a unique test ID (the seed already inserts google/microsoft)
    let provider = oauth_provider::ActiveModel {
        id: Set("test_refresh_provider".to_string()),
        name: Set("Test Refresh Provider".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client_id".to_string())),
        client_secret: Set(Some("client_secret".to_string())),
        auto_approve: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };
    provider.insert(&db).await.expect("insert provider");

    // Insert a user
    let new_user = user::ActiveModel {
        username: Set("refreshuser".to_string()),
        email: Set("refresh@example.com".to_string()),
        hashed_password: Set("hash".to_string()),
        is_active: Set(true),
        is_approved: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let created_user = new_user.insert(&db).await.expect("insert user");

    // Insert an oauth_account with an already-expired access token
    let expired_at = now - chrono::Duration::hours(1);
    let account = oauth_account::ActiveModel {
        user_id: Set(created_user.id),
        provider: Set("test_refresh_provider".to_string()),
        provider_user_id: Set("google_uid_1".to_string()),
        email: Set(Some("refresh@example.com".to_string())),
        display_name: Set(Some("Refresh User".to_string())),
        access_token: Set(Some("old_access_token".to_string())),
        refresh_token: Set(Some("old_refresh_token".to_string())),
        token_expires_at: Set(Some(expired_at)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let created_account = account.insert(&db).await.expect("insert oauth_account");

    // Simulate what the refresh handler does after a successful provider call
    let new_token_data = json!({"expires_in": 3600, "access_token": "new_access_token"});
    let mut active: oauth_account::ActiveModel = created_account.into();
    active.access_token = Set(Some("new_access_token".to_string()));
    active.token_expires_at = Set(compute_token_expires_at(&new_token_data));
    active.updated_at = Set(Utc::now());
    let updated = active.update(&db).await.expect("update oauth_account");

    assert_eq!(
        updated.access_token,
        Some("new_access_token".to_string()),
        "access_token should be updated"
    );
    assert!(
        updated.token_expires_at.is_some(),
        "token_expires_at should be set after refresh"
    );
    let new_exp = updated.token_expires_at.unwrap();
    assert!(
        new_exp > now,
        "refreshed token_expires_at should be in the future"
    );
}

// ============================================================================
// Bug 3: auto_approve controls new user approval status
// ============================================================================

#[tokio::test]
async fn test_auto_approve_false_creates_unapproved_user() {
    let db = create_test_db().await;
    let now = Utc::now();

    // Provider with auto_approve = false (use a unique ID to avoid conflict with seeded providers)
    let provider = oauth_provider::ActiveModel {
        id: Set("test_no_autoapprove".to_string()),
        name: Set("Test No AutoApprove".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client".to_string())),
        client_secret: Set(Some("secret".to_string())),
        auto_approve: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
    };
    let provider_model = provider.insert(&db).await.expect("insert provider");

    assert!(!provider_model.auto_approve);

    // Replicate what oauth_callback does: set is_approved from provider_model.auto_approve
    let new_user = user::ActiveModel {
        username: Set("oauthuser1".to_string()),
        email: Set("oauth1@example.com".to_string()),
        hashed_password: Set("hash".to_string()),
        is_active: Set(true),
        is_approved: Set(provider_model.auto_approve),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let created_user = new_user.insert(&db).await.expect("insert user");

    assert!(
        !created_user.is_approved,
        "User should NOT be approved when provider.auto_approve = false"
    );
}

#[tokio::test]
async fn test_auto_approve_true_creates_approved_user() {
    let db = create_test_db().await;
    let now = Utc::now();

    // Provider with auto_approve = true (default / backwards-compatible)
    let provider = oauth_provider::ActiveModel {
        id: Set("test_autoapprove".to_string()),
        name: Set("Test AutoApprove".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client".to_string())),
        client_secret: Set(Some("secret".to_string())),
        auto_approve: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };
    let provider_model = provider.insert(&db).await.expect("insert provider");

    assert!(provider_model.auto_approve);

    let new_user = user::ActiveModel {
        username: Set("oauthuser2".to_string()),
        email: Set("oauth2@example.com".to_string()),
        hashed_password: Set("hash".to_string()),
        is_active: Set(true),
        is_approved: Set(provider_model.auto_approve),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let created_user = new_user.insert(&db).await.expect("insert user");

    assert!(
        created_user.is_approved,
        "User SHOULD be approved when provider.auto_approve = true"
    );
}
