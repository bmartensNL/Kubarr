use kubarr::services::security::{
    create_access_token, create_refresh_token, decode_token, generate_random_string,
    generate_rsa_key_pair, generate_secure_password, hash_password, init_jwt_keys, verify_password,
};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};

mod common;
use common::create_test_db;

// ==========================================================================
// Password Hashing Tests
// ==========================================================================

#[test]
fn test_password_hashing() {
    let password = "test_password123";
    let hash = hash_password(password).unwrap();
    assert!(verify_password(password, &hash));
    assert!(!verify_password("wrong_password", &hash));
}

#[test]
fn test_password_hashing_empty_password() {
    let password = "";
    let hash = hash_password(password).unwrap();
    assert!(verify_password(password, &hash));
    assert!(!verify_password("not_empty", &hash));
}

#[test]
fn test_password_hashing_unicode() {
    let password = "пароль密码🔐";
    let hash = hash_password(password).unwrap();
    assert!(verify_password(password, &hash));
}

#[test]
fn test_password_hashing_long_password() {
    let password = "a".repeat(1000);
    let hash = hash_password(&password).unwrap();
    assert!(verify_password(&password, &hash));
}

#[test]
fn test_verify_password_invalid_hash() {
    assert!(!verify_password("test", "not_a_valid_hash"));
}

// ==========================================================================
// Random String Generation Tests
// ==========================================================================

#[test]
fn test_random_string() {
    let s1 = generate_random_string(16);
    let s2 = generate_random_string(16);
    assert_eq!(s1.len(), 32); // hex encoding doubles length
    assert_ne!(s1, s2);
}

#[test]
fn test_random_string_zero_length() {
    let s = generate_random_string(0);
    assert_eq!(s.len(), 0);
}

#[test]
fn test_random_string_large_length() {
    let s = generate_random_string(1000);
    assert_eq!(s.len(), 2000); // hex encoding doubles length
}

#[test]
fn test_secure_password_generation() {
    let password = generate_secure_password(20);
    assert_eq!(password.len(), 20);

    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    assert!(has_lower || has_upper);
}

// ==========================================================================
// RSA Key Generation Tests
// ==========================================================================

#[test]
fn test_generate_rsa_key_pair() {
    let result = generate_rsa_key_pair();
    assert!(result.is_ok());

    let (private_pem, public_pem) = result.unwrap();

    assert!(private_pem.contains("-----BEGIN PRIVATE KEY-----"));
    assert!(private_pem.contains("-----END PRIVATE KEY-----"));
    assert!(public_pem.contains("-----BEGIN PUBLIC KEY-----"));
    assert!(public_pem.contains("-----END PUBLIC KEY-----"));

    let private = RsaPrivateKey::from_pkcs8_pem(&private_pem);
    assert!(private.is_ok());

    let public = RsaPublicKey::from_public_key_pem(&public_pem);
    assert!(public.is_ok());
}

// ==========================================================================
// JWT Token Tests
// ==========================================================================

#[tokio::test]
async fn test_create_and_decode_access_token() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let token = create_access_token(
        "user123",
        Some("user@example.com"),
        Some("openid email"),
        Some("client_id"),
        Some(3600),
        Some(vec!["apps.view".to_string()]),
        Some(vec!["sonarr".to_string()]),
    )
    .expect("Failed to create access token");

    let claims = decode_token(&token).expect("Failed to decode access token");

    assert_eq!(claims.sub, "user123");
    assert_eq!(claims.email, Some("user@example.com".to_string()));
    assert_eq!(claims.scope, Some("openid email".to_string()));
    assert_eq!(claims.client_id, Some("client_id".to_string()));
    assert_eq!(claims.permissions, Some(vec!["apps.view".to_string()]));
    assert_eq!(claims.allowed_apps, Some(vec!["sonarr".to_string()]));
    assert!(claims.token_type.is_none());
}

#[tokio::test]
async fn test_create_and_decode_refresh_token() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let token = create_refresh_token(
        "user123",
        Some("user@example.com"),
        Some("openid"),
        Some("client_id"),
        None,
    )
    .expect("Failed to create refresh token");

    let claims = decode_token(&token).expect("Failed to decode refresh token");

    assert_eq!(claims.sub, "user123");
    assert_eq!(claims.token_type, Some("refresh".to_string()));
    assert!(claims.permissions.is_none());
    assert!(claims.allowed_apps.is_none());
}

#[tokio::test]
async fn test_token_expiration() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let token = create_access_token(
        "user123",
        None,
        None,
        None,
        Some(-10), // Expired 10 seconds ago
        None,
        None,
    )
    .expect("Failed to create expired token");

    let result = decode_token(&token);
    assert!(
        result.is_err(),
        "Expected token to be expired but decode succeeded"
    );
}

#[tokio::test]
async fn test_decode_invalid_token() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let result = decode_token("not.a.valid.token");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_decode_malformed_token() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let result = decode_token("completely_invalid");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_access_token_minimal() {
    let db = create_test_db().await;
    init_jwt_keys(&db).await.expect("Failed to init JWT keys");

    let token = create_access_token("user123", None, None, None, None, None, None);

    assert!(token.is_ok());
    let claims = decode_token(&token.unwrap()).unwrap();
    assert_eq!(claims.sub, "user123");
    assert!(claims.email.is_none());
    assert!(claims.scope.is_none());
}
