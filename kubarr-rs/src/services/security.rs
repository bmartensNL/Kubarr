use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::Rng;
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;

use crate::config::CONFIG;
use crate::error::{AppError, Result};

// JWT token expiration times (in seconds)
const ACCESS_TOKEN_EXPIRE: i64 = 3600; // 1 hour
const REFRESH_TOKEN_EXPIRE: i64 = 604800; // 7 days

// In-memory key cache
static PRIVATE_KEY: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));
static PUBLIC_KEY: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

/// JWT token claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // Subject (user identifier)
    pub iss: String, // Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>, // Audience (client_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>, // User email
    pub exp: i64,    // Expiration time
    pub iat: i64,    // Issued at
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>, // JWT ID for uniqueness
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>, // "refresh" for refresh tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>, // OAuth2 scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>, // OAuth2 client ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>, // User permissions (e.g., ["apps.view", "app.sonarr"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_apps: Option<Vec<String>>, // Apps user can access (e.g., ["sonarr", "radarr", "*"])
}

/// Get the JWT private key (PEM format)
pub fn get_private_key() -> Result<String> {
    // Fast path: check cache with read lock
    {
        let cache = PRIVATE_KEY.read();
        if let Some(key) = cache.as_ref() {
            return Ok(key.clone());
        }
    }

    // Slow path: acquire write lock with double-checked locking
    let mut priv_cache = PRIVATE_KEY.write();

    // Double-check: another thread might have initialized while we waited
    if let Some(key) = priv_cache.as_ref() {
        return Ok(key.clone());
    }

    // Try to load from file
    if CONFIG.jwt_private_key_path.exists() {
        let content = fs::read_to_string(&CONFIG.jwt_private_key_path)
            .map_err(|e| AppError::Internal(format!("Failed to read private key: {}", e)))?;

        if !content.trim().is_empty() {
            *priv_cache = Some(content.clone());
            return Ok(content);
        }
    }

    // Generate in-memory key for development
    tracing::warn!("JWT private key not found, generating temporary key");
    let (private_pem, public_pem) = generate_rsa_key_pair()?;

    *priv_cache = Some(private_pem.clone());
    drop(priv_cache); // Release private key lock before acquiring public key lock

    {
        let mut pub_cache = PUBLIC_KEY.write();
        // Only set if not already set
        if pub_cache.is_none() {
            *pub_cache = Some(public_pem);
        }
    }

    Ok(private_pem)
}

/// Get the JWT public key (PEM format)
pub fn get_public_key() -> Result<String> {
    // Fast path: check cache with read lock
    {
        let cache = PUBLIC_KEY.read();
        if let Some(key) = cache.as_ref() {
            return Ok(key.clone());
        }
    }

    // Slow path: acquire write lock with double-checked locking
    let mut pub_cache = PUBLIC_KEY.write();

    // Double-check: another thread might have initialized while we waited
    if let Some(key) = pub_cache.as_ref() {
        return Ok(key.clone());
    }

    // Try to load from file
    if CONFIG.jwt_public_key_path.exists() {
        let content = fs::read_to_string(&CONFIG.jwt_public_key_path)
            .map_err(|e| AppError::Internal(format!("Failed to read public key: {}", e)))?;

        if !content.trim().is_empty() {
            *pub_cache = Some(content.clone());
            return Ok(content);
        }
    }

    // Release lock before calling get_private_key to avoid deadlock
    drop(pub_cache);

    // Trigger private key generation which also generates public key
    get_private_key()?;

    let cache = PUBLIC_KEY.read();
    cache
        .clone()
        .ok_or_else(|| AppError::Internal("Public key not available".to_string()))
}

/// Generate an RSA key pair for JWT signing
pub fn generate_rsa_key_pair() -> Result<(String, String)> {
    let mut rng = rand::thread_rng();

    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|e| AppError::Internal(format!("Failed to generate RSA key: {}", e)))?;

    let public_key = RsaPublicKey::from(&private_key);

    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AppError::Internal(format!("Failed to serialize private key: {}", e)))?
        .to_string();

    let public_pem = public_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AppError::Internal(format!("Failed to serialize public key: {}", e)))?;

    Ok((private_pem, public_pem))
}

/// Hash a password using bcrypt
pub fn hash_password(password: &str) -> Result<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

/// Verify a password against its hash
pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Create a JWT access token
pub fn create_access_token(
    subject: &str,
    email: Option<&str>,
    scope: Option<&str>,
    client_id: Option<&str>,
    expires_in: Option<i64>,
    permissions: Option<Vec<String>>,
    allowed_apps: Option<Vec<String>>,
) -> Result<String> {
    let now = Utc::now();
    let exp = now + Duration::seconds(expires_in.unwrap_or(ACCESS_TOKEN_EXPIRE));

    let issuer = format!("{}/auth", CONFIG.oauth2_issuer_url);
    let claims = Claims {
        sub: subject.to_string(),
        iss: issuer,
        aud: client_id.map(String::from),
        email: email.map(String::from),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: None,
        scope: scope.map(String::from),
        client_id: client_id.map(String::from),
        permissions,
        allowed_apps,
    };

    let private_key = get_private_key()?;
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid private key: {}", e)))?;

    let header = Header::new(jsonwebtoken::Algorithm::RS256);
    encode(&header, &claims, &encoding_key).map_err(|e| e.into())
}

/// Create a JWT refresh token (no permissions embedded - only for token refresh)
pub fn create_refresh_token(
    subject: &str,
    email: Option<&str>,
    scope: Option<&str>,
    client_id: Option<&str>,
    expires_in: Option<i64>,
) -> Result<String> {
    let now = Utc::now();
    let exp = now + Duration::seconds(expires_in.unwrap_or(REFRESH_TOKEN_EXPIRE));

    let issuer = format!("{}/auth", CONFIG.oauth2_issuer_url);
    let claims = Claims {
        sub: subject.to_string(),
        iss: issuer,
        aud: client_id.map(String::from),
        email: email.map(String::from),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: Some(uuid::Uuid::new_v4().to_string()),
        token_type: Some("refresh".to_string()),
        scope: scope.map(String::from),
        client_id: client_id.map(String::from),
        permissions: None, // Refresh tokens don't carry permissions
        allowed_apps: None,
    };

    let private_key = get_private_key()?;
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid private key: {}", e)))?;

    let header = Header::new(jsonwebtoken::Algorithm::RS256);
    encode(&header, &claims, &encoding_key).map_err(|e| e.into())
}

/// Decode and validate a JWT token
pub fn decode_token(token: &str) -> Result<Claims> {
    let public_key = get_public_key()?;
    let decoding_key = DecodingKey::from_rsa_pem(public_key.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid public key: {}", e)))?;

    let mut validation = Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.validate_exp = true;
    // Disable audience validation - audience is application-specific
    validation.validate_aud = false;
    // No clock skew tolerance for expiration check
    validation.leeway = 0;

    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

/// Generate a cryptographically secure random string (hex)
pub fn generate_random_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..length).map(|_| rng.gen()).collect();
    hex::encode(bytes)
}

/// Generate an OAuth2 authorization code
pub fn generate_authorization_code() -> String {
    generate_random_string(32)
}

/// Hash an OAuth2 client secret (same as password hashing)
pub fn hash_client_secret(secret: &str) -> Result<String> {
    hash_password(secret)
}

/// Verify an OAuth2 client secret
pub fn verify_client_secret(plain_secret: &str, hashed_secret: &str) -> bool {
    verify_password(plain_secret, hashed_secret)
}

/// Verify PKCE code challenge
pub fn verify_pkce(code_verifier: &str, code_challenge: &str, method: &str) -> bool {
    match method {
        "S256" => {
            let mut hasher = Sha256::new();
            hasher.update(code_verifier.as_bytes());
            let digest = hasher.finalize();
            let computed_challenge = URL_SAFE_NO_PAD.encode(digest);
            computed_challenge == code_challenge
        }
        "plain" => code_verifier == code_challenge,
        _ => false,
    }
}

/// Generate a secure cookie secret for oauth2-proxy (base64-encoded 32 bytes)
pub fn generate_cookie_secret() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Generate a secure random password
pub fn generate_secure_password(length: usize) -> String {
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Get the JWKS (JSON Web Key Set) for the public key
pub fn get_jwks() -> Result<serde_json::Value> {
    let public_pem = get_public_key()?;

    // Parse the public key
    let public_key = RsaPublicKey::from_public_key_pem(&public_pem)
        .map_err(|e| AppError::Internal(format!("Failed to parse public key: {}", e)))?;

    // Get the modulus and exponent
    use rsa::traits::PublicKeyParts;
    let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());

    Ok(serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "n": n,
            "e": e,
            "kid": "kubarr-jwt-key"
        }]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Invalid bcrypt hash should return false, not panic
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
        let decoded = base64::engine::general_purpose::STANDARD.decode(&secret1);
        assert!(decoded.is_ok());
        assert_eq!(decoded.unwrap().len(), 32);
    }

    #[test]
    fn test_secure_password_generation() {
        let password = generate_secure_password(20);
        assert_eq!(password.len(), 20);

        // Verify it contains at least some variety
        let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
        let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
        // Note: May not always have special chars due to randomness
        assert!(has_lower || has_upper);
    }

    // ==========================================================================
    // PKCE Tests
    // ==========================================================================

    #[test]
    fn test_pkce_s256() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // This is the base64url-encoded SHA256 hash of the verifier
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

        // Verify PEM format
        assert!(private_pem.contains("-----BEGIN PRIVATE KEY-----"));
        assert!(private_pem.contains("-----END PRIVATE KEY-----"));
        assert!(public_pem.contains("-----BEGIN PUBLIC KEY-----"));
        assert!(public_pem.contains("-----END PUBLIC KEY-----"));

        // Verify keys can be parsed
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
        // Refresh tokens should NOT have permissions
        assert!(claims.permissions.is_none());
        assert!(claims.allowed_apps.is_none());
    }

    #[test]
    fn test_token_expiration() {
        // Create a token that expires in the past
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

        // Decoding should fail due to expiration
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
        // Test with minimal parameters
        let token = create_access_token("user123", None, None, None, None, None, None);

        assert!(token.is_ok());
        let claims = decode_token(&token.unwrap()).unwrap();
        assert_eq!(claims.sub, "user123");
        assert!(claims.email.is_none());
        assert!(claims.scope.is_none());
    }

    // ==========================================================================
    // JWKS Tests
    // ==========================================================================

    #[test]
    fn test_get_jwks() {
        let jwks = get_jwks();
        assert!(jwks.is_ok());

        let jwks = jwks.unwrap();

        // Verify structure
        assert!(jwks.get("keys").is_some());
        let keys = jwks.get("keys").unwrap().as_array().unwrap();
        assert_eq!(keys.len(), 1);

        let key = &keys[0];
        assert_eq!(key.get("kty").unwrap(), "RSA");
        assert_eq!(key.get("use").unwrap(), "sig");
        assert_eq!(key.get("alg").unwrap(), "RS256");
        assert_eq!(key.get("kid").unwrap(), "kubarr-jwt-key");
        assert!(key.get("n").is_some()); // modulus
        assert!(key.get("e").is_some()); // exponent
    }

    // ==========================================================================
    // Claims Serialization Tests
    // ==========================================================================

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "user123".to_string(),
            iss: "https://example.com".to_string(),
            aud: Some("client_id".to_string()),
            email: Some("user@example.com".to_string()),
            exp: 1234567890,
            iat: 1234567800,
            jti: Some("unique-id".to_string()),
            token_type: None,
            scope: Some("openid".to_string()),
            client_id: Some("client_id".to_string()),
            permissions: Some(vec!["apps.view".to_string()]),
            allowed_apps: Some(vec!["*".to_string()]),
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(json.contains("user123"));
        assert!(json.contains("user@example.com"));

        // token_type is None, so it should not appear in JSON
        assert!(!json.contains("token_type"));

        // Deserialize back
        let decoded: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.email, claims.email);
    }

    #[test]
    fn test_claims_optional_fields() {
        let claims = Claims {
            sub: "user123".to_string(),
            iss: "https://example.com".to_string(),
            aud: None,
            email: None,
            exp: 1234567890,
            iat: 1234567800,
            jti: None,
            token_type: None,
            scope: None,
            client_id: None,
            permissions: None,
            allowed_apps: None,
        };

        let json = serde_json::to_string(&claims).unwrap();

        // Optional None fields should not appear in JSON
        assert!(!json.contains("aud"));
        assert!(!json.contains("email"));
        assert!(!json.contains("jti"));
        assert!(!json.contains("token_type"));
        assert!(!json.contains("scope"));
        assert!(!json.contains("permissions"));
        assert!(!json.contains("allowed_apps"));
    }
}
