use kubarr::services::security::{
    create_access_token, create_refresh_token, decode_token, generate_authorization_code,
    generate_cookie_secret, generate_random_string, generate_rsa_key_pair,
    generate_secure_password, hash_client_secret, hash_password, verify_client_secret,
    verify_password, verify_pkce,
};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{RsaPrivateKey, RsaPublicKey};

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

#[test]
fn test_client_secret_hashing() {
    let secret = "super_secret_client_secret_123";
    let hash = hash_client_secret(secret).unwrap();
    assert!(verify_client_secret(secret, &hash));
    assert!(!verify_client_secret("wrong_secret", &hash));
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
fn test_authorization_code_generation() {
    let code1 = generate_authorization_code();
    let code2 = generate_authorization_code();
    assert_eq!(code1.len(), 64); // 32 bytes * 2 (hex)
    assert_ne!(code1, code2);
}

#[test]
fn test_cookie_secret_generation() {
    let secret1 = generate_cookie_secret();
    let secret2 = generate_cookie_secret();

    // Base64-encoded 32 bytes should be 44 chars (with padding)
    assert_eq!(secret1.len(), 44);
    assert_ne!(secret1, secret2);

    // Verify it's valid base64
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &secret1);
    assert!(decoded.is_ok());
    assert_eq!(decoded.unwrap().len(), 32);
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
// PKCE Tests
// ==========================================================================

#[test]
fn test_pkce_s256() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    assert!(verify_pkce(verifier, challenge, "S256"));
}

#[test]
fn test_pkce_s256_wrong_challenge() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let wrong_challenge = "wrong_challenge_value";
    assert!(!verify_pkce(verifier, wrong_challenge, "S256"));
}

#[test]
fn test_pkce_plain() {
    let verifier = "test_verifier";
    assert!(verify_pkce(verifier, verifier, "plain"));
    assert!(!verify_pkce(verifier, "different", "plain"));
}

#[test]
fn test_pkce_unsupported_method() {
    assert!(!verify_pkce("verifier", "challenge", "unsupported"));
    assert!(!verify_pkce("verifier", "challenge", ""));
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

#[test]
fn test_create_and_decode_access_token() {
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

#[test]
fn test_create_and_decode_refresh_token() {
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

#[test]
fn test_token_expiration() {
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

#[test]
fn test_decode_invalid_token() {
    let result = decode_token("not.a.valid.token");
    assert!(result.is_err());
}

#[test]
fn test_decode_malformed_token() {
    let result = decode_token("completely_invalid");
    assert!(result.is_err());
}

#[test]
fn test_access_token_minimal() {
    let token = create_access_token("user123", None, None, None, None, None, None);

    assert!(token.is_ok());
    let claims = decode_token(&token.unwrap()).unwrap();
    assert_eq!(claims.sub, "user123");
    assert!(claims.email.is_none());
    assert!(claims.scope.is_none());
}
