//! Tests for OAuth2 service

use kubarr::services::oauth2::{OAuth2Service, TokenIntrospection, TokenPair};
use kubarr::test_helpers::{create_test_db_with_seed, create_test_user_with_role};

// ==========================================================================
// TokenPair Tests
// ==========================================================================

#[test]
fn test_token_pair_debug() {
    let pair = TokenPair {
        access_token: "access123".to_string(),
        refresh_token: "refresh456".to_string(),
        expires_in: 3600,
        refresh_expires_in: 604800,
        scope: Some("openid profile".to_string()),
    };

    let debug_str = format!("{:?}", pair);
    assert!(debug_str.contains("access123"));
    assert!(debug_str.contains("refresh456"));
    assert!(debug_str.contains("3600"));
}

#[test]
fn test_token_pair_clone() {
    let pair = TokenPair {
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        expires_in: 3600,
        refresh_expires_in: 604800,
        scope: None,
    };

    let cloned = pair.clone();
    assert_eq!(pair.access_token, cloned.access_token);
    assert_eq!(pair.refresh_token, cloned.refresh_token);
    assert_eq!(pair.expires_in, cloned.expires_in);
}

// ==========================================================================
// TokenIntrospection Tests
// ==========================================================================

#[test]
fn test_token_introspection_inactive() {
    let introspection = TokenIntrospection::inactive();

    assert!(!introspection.active);
    assert!(introspection.sub.is_none());
    assert!(introspection.username.is_none());
    assert!(introspection.email.is_none());
    assert!(introspection.scope.is_none());
    assert!(introspection.exp.is_none());
    assert!(introspection.client_id.is_none());
}

#[test]
fn test_token_introspection_active_serialize() {
    let introspection = TokenIntrospection {
        active: true,
        sub: Some("123".to_string()),
        username: Some("testuser".to_string()),
        email: Some("test@example.com".to_string()),
        scope: Some("openid".to_string()),
        exp: Some(1700000000),
        client_id: Some("my-client".to_string()),
    };

    let json = serde_json::to_string(&introspection).unwrap();
    assert!(json.contains("\"active\":true"));
    assert!(json.contains("\"sub\":\"123\""));
    assert!(json.contains("\"username\":\"testuser\""));
    assert!(json.contains("\"email\":\"test@example.com\""));
    assert!(json.contains("\"scope\":\"openid\""));
    assert!(json.contains("\"client_id\":\"my-client\""));
}

#[test]
fn test_token_introspection_inactive_serialize_skips_none() {
    let introspection = TokenIntrospection::inactive();
    let json = serde_json::to_string(&introspection).unwrap();

    // Should only have "active": false
    assert!(json.contains("\"active\":false"));
    // None values should be skipped
    assert!(!json.contains("\"sub\""));
    assert!(!json.contains("\"username\""));
    assert!(!json.contains("\"email\""));
}

#[test]
fn test_token_introspection_clone() {
    let introspection = TokenIntrospection {
        active: true,
        sub: Some("123".to_string()),
        username: Some("user".to_string()),
        email: None,
        scope: None,
        exp: Some(123456),
        client_id: None,
    };

    let cloned = introspection.clone();
    assert_eq!(introspection.active, cloned.active);
    assert_eq!(introspection.sub, cloned.sub);
    assert_eq!(introspection.exp, cloned.exp);
}

// ==========================================================================
// OAuth2Service Tests
// ==========================================================================

#[tokio::test]
async fn test_oauth2_service_get_nonexistent_client() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    let client = service.get_client("nonexistent").await.unwrap();
    assert!(client.is_none());
}

#[tokio::test]
async fn test_oauth2_service_validate_nonexistent_client() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    let valid = service
        .validate_client("nonexistent", Some("secret"))
        .await
        .unwrap();
    assert!(!valid);
}

#[tokio::test]
async fn test_oauth2_service_create_and_get_client() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    // Create a client
    let client = service
        .create_client(
            "test-client",
            "test-secret",
            "Test Client",
            &["http://localhost:8080/callback".to_string()],
        )
        .await
        .unwrap();

    assert_eq!(client.client_id, "test-client");
    assert_eq!(client.name, "Test Client");

    // Retrieve the client
    let retrieved = service.get_client("test-client").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Client");
}

#[tokio::test]
async fn test_oauth2_service_validate_client_secret() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    // Create a client
    service
        .create_client(
            "my-client",
            "my-secret",
            "My Client",
            &["http://localhost/callback".to_string()],
        )
        .await
        .unwrap();

    // Valid secret
    let valid = service
        .validate_client("my-client", Some("my-secret"))
        .await
        .unwrap();
    assert!(valid);

    // Invalid secret
    let invalid = service
        .validate_client("my-client", Some("wrong-secret"))
        .await
        .unwrap();
    assert!(!invalid);

    // No secret (public client)
    let no_secret = service.validate_client("my-client", None).await.unwrap();
    assert!(no_secret);
}

#[tokio::test]
async fn test_oauth2_service_create_authorization_code() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    // Create client and user
    service
        .create_client(
            "auth-client",
            "secret",
            "Auth Client",
            &["http://localhost/callback".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "authuser", "auth@test.com", "password", "admin").await;

    // Create authorization code
    let code = service
        .create_authorization_code(
            "auth-client",
            user.id,
            "http://localhost/callback",
            Some("openid"),
            None,
            None,
            300, // 5 minutes
        )
        .await
        .unwrap();

    assert!(!code.is_empty());
    assert!(code.len() > 20); // Should be a reasonably long random string
}

#[tokio::test]
async fn test_oauth2_service_validate_authorization_code() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    // Create client and user
    service
        .create_client(
            "val-client",
            "secret",
            "Validation Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "valuser", "val@test.com", "password", "admin").await;

    // Create authorization code
    let code = service
        .create_authorization_code(
            "val-client",
            user.id,
            "http://localhost/cb",
            Some("openid"),
            None,
            None,
            300,
        )
        .await
        .unwrap();

    // Validate the code
    let validated = service
        .validate_authorization_code(&code, "val-client", "http://localhost/cb", None)
        .await
        .unwrap();

    assert!(validated.is_some());
    let auth_code = validated.unwrap();
    assert_eq!(auth_code.user_id, user.id);

    // Try to use the code again (should fail - already used)
    let reused = service
        .validate_authorization_code(&code, "val-client", "http://localhost/cb", None)
        .await
        .unwrap();
    assert!(reused.is_none());
}

#[tokio::test]
async fn test_oauth2_service_validate_code_wrong_client() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "client-a",
            "secret",
            "Client A",
            &["http://a/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "user1", "user1@test.com", "password", "admin").await;

    let code = service
        .create_authorization_code("client-a", user.id, "http://a/cb", None, None, None, 300)
        .await
        .unwrap();

    // Try to validate with wrong client ID
    let result = service
        .validate_authorization_code(&code, "wrong-client", "http://a/cb", None)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_oauth2_service_validate_code_wrong_redirect() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "redir-client",
            "secret",
            "Redirect Client",
            &["http://correct/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "user2", "user2@test.com", "password", "admin").await;

    let code = service
        .create_authorization_code(
            "redir-client",
            user.id,
            "http://correct/cb",
            None,
            None,
            None,
            300,
        )
        .await
        .unwrap();

    // Try to validate with wrong redirect URI
    let result = service
        .validate_authorization_code(&code, "redir-client", "http://wrong/cb", None)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_oauth2_service_create_tokens() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "token-client",
            "secret",
            "Token Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "tokenuser", "token@test.com", "password", "admin").await;

    let tokens = service
        .create_tokens(
            "token-client",
            user.id,
            Some("openid profile"),
            3600,
            604800,
        )
        .await
        .unwrap();

    assert!(!tokens.access_token.is_empty());
    assert!(!tokens.refresh_token.is_empty());
    assert_eq!(tokens.expires_in, 3600);
    assert_eq!(tokens.refresh_expires_in, 604800);
    assert_eq!(tokens.scope, Some("openid profile".to_string()));
}

#[tokio::test]
async fn test_oauth2_service_validate_access_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "access-client",
            "secret",
            "Access Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "accessuser", "access@test.com", "password", "admin").await;

    let tokens = service
        .create_tokens("access-client", user.id, None, 3600, 604800)
        .await
        .unwrap();

    // Validate the access token
    let validated = service
        .validate_access_token(&tokens.access_token)
        .await
        .unwrap();
    assert!(validated.is_some());

    // Invalid token should return None
    let invalid = service
        .validate_access_token("invalid-token")
        .await
        .unwrap();
    assert!(invalid.is_none());
}

#[tokio::test]
async fn test_oauth2_service_revoke_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "revoke-client",
            "secret",
            "Revoke Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "revokeuser", "revoke@test.com", "password", "admin").await;

    let tokens = service
        .create_tokens("revoke-client", user.id, None, 3600, 604800)
        .await
        .unwrap();

    // Validate before revoke
    let valid_before = service
        .validate_access_token(&tokens.access_token)
        .await
        .unwrap();
    assert!(valid_before.is_some());

    // Revoke the token
    let revoked = service.revoke_token(&tokens.access_token).await.unwrap();
    assert!(revoked);

    // Validate after revoke
    let valid_after = service
        .validate_access_token(&tokens.access_token)
        .await
        .unwrap();
    assert!(valid_after.is_none());
}

#[tokio::test]
async fn test_oauth2_service_revoke_nonexistent_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    let revoked = service.revoke_token("nonexistent-token").await.unwrap();
    assert!(!revoked);
}

#[tokio::test]
async fn test_oauth2_service_introspect_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "intro-client",
            "secret",
            "Introspect Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "introuser", "intro@test.com", "password", "admin").await;

    let tokens = service
        .create_tokens("intro-client", user.id, Some("openid"), 3600, 604800)
        .await
        .unwrap();

    // Introspect the token
    let introspection = service
        .introspect_token(&tokens.access_token)
        .await
        .unwrap();

    assert!(introspection.active);
    assert_eq!(introspection.sub, Some(user.id.to_string()));
    assert_eq!(introspection.username, Some("introuser".to_string()));
    assert_eq!(introspection.email, Some("intro@test.com".to_string()));
    assert_eq!(introspection.scope, Some("openid".to_string()));
    assert_eq!(introspection.client_id, Some("intro-client".to_string()));
}

#[tokio::test]
async fn test_oauth2_service_introspect_invalid_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    let introspection = service.introspect_token("invalid-token").await.unwrap();
    assert!(!introspection.active);
}

#[tokio::test]
async fn test_oauth2_service_refresh_access_token() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "refresh-client",
            "secret",
            "Refresh Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "refreshuser", "refresh@test.com", "password", "admin")
            .await;

    let original_tokens = service
        .create_tokens("refresh-client", user.id, Some("openid"), 3600, 604800)
        .await
        .unwrap();

    // Refresh the token
    let new_tokens = service
        .refresh_access_token(&original_tokens.refresh_token, "refresh-client")
        .await
        .unwrap();

    assert!(new_tokens.is_some());
    let new_pair = new_tokens.unwrap();
    assert!(!new_pair.access_token.is_empty());
    assert_ne!(new_pair.access_token, original_tokens.access_token);

    // Original access token should be revoked
    let old_valid = service
        .validate_access_token(&original_tokens.access_token)
        .await
        .unwrap();
    assert!(old_valid.is_none());
}

#[tokio::test]
async fn test_oauth2_service_refresh_wrong_client() {
    let db = create_test_db_with_seed().await;
    let service = OAuth2Service::new(&db);

    service
        .create_client(
            "orig-client",
            "secret",
            "Original Client",
            &["http://localhost/cb".to_string()],
        )
        .await
        .unwrap();

    let user =
        create_test_user_with_role(&db, "origuser", "orig@test.com", "password", "admin").await;

    let tokens = service
        .create_tokens("orig-client", user.id, None, 3600, 604800)
        .await
        .unwrap();

    // Try to refresh with wrong client ID
    let result = service
        .refresh_access_token(&tokens.refresh_token, "wrong-client")
        .await
        .unwrap();
    assert!(result.is_none());
}
