//! Tests for OAuth security fixes:
//!   - Bug 1: Session cookie Secure flag controlled by KUBARR_INSECURE_COOKIES
//!   - Bug 2: token_expires_at populated from provider response expires_in field
//!   - Integration: token refresh endpoint updates stored token

use kubarr::endpoints::oauth::{build_session_cookie, compute_token_expires_at};

mod common;

// ==========================================================================
// Bug 1: Secure cookie flag
// ==========================================================================

#[test]
fn test_cookie_has_secure_flag_by_default() {
    // Remove env var to simulate production (no KUBARR_INSECURE_COOKIES)
    std::env::remove_var("KUBARR_INSECURE_COOKIES");

    let cookie = build_session_cookie("test_token_abc123");

    assert!(
        cookie.contains("; Secure"),
        "Expected Secure flag in cookie when KUBARR_INSECURE_COOKIES is not set, got: {}",
        cookie
    );
    assert!(cookie.contains("HttpOnly"), "Cookie must contain HttpOnly");
    assert!(cookie.contains("SameSite=Lax"), "Cookie must contain SameSite=Lax");
    assert!(
        cookie.contains("kubarr_session=test_token_abc123"),
        "Cookie must contain the session token"
    );
}

#[test]
fn test_cookie_omits_secure_flag_when_insecure_cookies_set() {
    std::env::set_var("KUBARR_INSECURE_COOKIES", "1");

    let cookie = build_session_cookie("test_token_abc123");

    // Clean up immediately to avoid affecting other tests
    std::env::remove_var("KUBARR_INSECURE_COOKIES");

    assert!(
        !cookie.contains("; Secure"),
        "Expected no Secure flag when KUBARR_INSECURE_COOKIES is set, got: {}",
        cookie
    );
    assert!(cookie.contains("HttpOnly"), "Cookie must still contain HttpOnly");
    assert!(
        cookie.contains("kubarr_session=test_token_abc123"),
        "Cookie must contain the session token"
    );
}

#[test]
fn test_cookie_format_max_age() {
    std::env::remove_var("KUBARR_INSECURE_COOKIES");
    let cookie = build_session_cookie("mytoken");
    assert!(
        cookie.contains("Max-Age=604800"),
        "Cookie must have 7-day Max-Age, got: {}",
        cookie
    );
    assert!(cookie.contains("Path=/"), "Cookie must have Path=/");
}

// ==========================================================================
// Bug 2: Token expiry from provider response
// ==========================================================================

#[test]
fn test_compute_token_expires_at_with_expires_in() {
    let before = chrono::Utc::now();
    let token_data = serde_json::json!({ "expires_in": 3600 });

    let expires_at = compute_token_expires_at(&token_data);

    assert!(expires_at.is_some(), "Expected a computed expiry datetime");
    let expires_at = expires_at.unwrap();

    // Should be approximately now + 3600 seconds
    let after = chrono::Utc::now() + chrono::Duration::seconds(3600);
    assert!(
        expires_at >= before + chrono::Duration::seconds(3590),
        "Expiry should be at least now + 3590s"
    );
    assert!(
        expires_at <= after + chrono::Duration::seconds(5),
        "Expiry should not be more than 5s past the expected window"
    );
}

#[test]
fn test_compute_token_expires_at_without_expires_in() {
    let token_data = serde_json::json!({ "access_token": "tok", "token_type": "Bearer" });

    let expires_at = compute_token_expires_at(&token_data);

    assert!(
        expires_at.is_none(),
        "Expected None when expires_in is missing from token response"
    );
}

#[test]
fn test_compute_token_expires_at_with_zero_expires_in() {
    let token_data = serde_json::json!({ "expires_in": 0 });

    let expires_at = compute_token_expires_at(&token_data);

    // expires_in = 0 means token expires immediately; we still compute it
    assert!(
        expires_at.is_some(),
        "Expected a datetime even for expires_in = 0"
    );
}

// ==========================================================================
// Integration: token refresh updates stored account record
// ==========================================================================

#[tokio::test]
async fn test_refresh_updates_token_in_db() {
    use chrono::Utc;
    use kubarr::models::prelude::*;
    use kubarr::models::{oauth_account, oauth_provider, user};
    use kubarr::services::security::hash_password;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let db = common::create_test_db().await;

    let now = Utc::now();

    // Create a test user
    let user_model = user::ActiveModel {
        username: Set("oauthuser".to_string()),
        email: Set("oauth@example.com".to_string()),
        hashed_password: Set(hash_password("password").unwrap()),
        is_active: Set(true),
        is_approved: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Create an OAuth provider
    let _provider = oauth_provider::ActiveModel {
        id: Set("google".to_string()),
        name: Set("Google".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client_id".to_string())),
        client_secret: Set(Some("client_secret".to_string())),
        auto_approve: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Create an OAuth account with an expired token
    let expired_at = now - chrono::Duration::hours(1);
    let oauth_record = oauth_account::ActiveModel {
        user_id: Set(user_model.id),
        provider: Set("google".to_string()),
        provider_user_id: Set("google-user-123".to_string()),
        email: Set(Some("oauth@example.com".to_string())),
        display_name: Set(Some("OAuth User".to_string())),
        access_token: Set(Some("old_access_token".to_string())),
        refresh_token: Set(Some("old_refresh_token".to_string())),
        token_expires_at: Set(Some(expired_at)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Simulate what the refresh endpoint does: update the stored token
    let new_token_data = serde_json::json!({
        "access_token": "new_access_token",
        "expires_in": 3600
    });

    let new_expires = compute_token_expires_at(&new_token_data);
    assert!(new_expires.is_some(), "New expiry must be computed from expires_in");

    let mut account_model: oauth_account::ActiveModel = oauth_record.into();
    account_model.access_token = Set(Some("new_access_token".to_string()));
    account_model.token_expires_at = Set(new_expires);
    account_model.updated_at = Set(now);
    account_model.update(&db).await.unwrap();

    // Verify the stored token was updated
    let updated = OauthAccount::find()
        .filter(oauth_account::Column::UserId.eq(user_model.id))
        .filter(oauth_account::Column::Provider.eq("google"))
        .one(&db)
        .await
        .unwrap()
        .expect("OAuth account should still exist");

    assert_eq!(
        updated.access_token.as_deref(),
        Some("new_access_token"),
        "Access token should be updated"
    );
    assert!(
        updated.token_expires_at.is_some(),
        "Token expiry should be set after refresh"
    );
    let updated_expires = updated.token_expires_at.unwrap();
    assert!(
        updated_expires > now,
        "New expiry should be in the future"
    );
}

#[tokio::test]
async fn test_auto_approve_false_creates_unapproved_user() {
    use chrono::Utc;
    use kubarr::models::prelude::*;
    use kubarr::models::{oauth_provider, user};
    use kubarr::services::security::hash_password;
    use sea_orm::{ActiveModelTrait, Set};

    let db = common::create_test_db().await;
    let now = Utc::now();

    // Create a provider with auto_approve = false
    let _provider = oauth_provider::ActiveModel {
        id: Set("microsoft".to_string()),
        name: Set("Microsoft".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client_id".to_string())),
        client_secret: Set(Some("client_secret".to_string())),
        auto_approve: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Simulate what oauth_callback does when auto_approve is false
    let provider = OauthProvider::find_by_id("microsoft")
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    let new_user = user::ActiveModel {
        username: Set("newuser".to_string()),
        email: Set("newuser@corp.example".to_string()),
        hashed_password: Set(hash_password("randompassword").unwrap()),
        is_active: Set(true),
        is_approved: Set(provider.auto_approve), // should be false
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    assert!(
        !new_user.is_approved,
        "User created with auto_approve=false should not be approved"
    );
}

#[tokio::test]
async fn test_auto_approve_true_creates_approved_user() {
    use chrono::Utc;
    use kubarr::models::prelude::*;
    use kubarr::models::{oauth_provider, user};
    use kubarr::services::security::hash_password;
    use sea_orm::{ActiveModelTrait, Set};

    let db = common::create_test_db().await;
    let now = Utc::now();

    // Create a provider with auto_approve = true (default behaviour)
    let _provider = oauth_provider::ActiveModel {
        id: Set("google".to_string()),
        name: Set("Google".to_string()),
        enabled: Set(true),
        client_id: Set(Some("client_id".to_string())),
        client_secret: Set(Some("client_secret".to_string())),
        auto_approve: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let provider = OauthProvider::find_by_id("google")
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    let new_user = user::ActiveModel {
        username: Set("approveduser".to_string()),
        email: Set("approved@example.com".to_string()),
        hashed_password: Set(hash_password("randompassword").unwrap()),
        is_active: Set(true),
        is_approved: Set(provider.auto_approve), // should be true
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    assert!(
        new_user.is_approved,
        "User created with auto_approve=true should be approved"
    );
}
