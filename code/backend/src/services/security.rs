use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::Rng;
use rsa::{
    pkcs8::{DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
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
#[allow(dead_code)]
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

// ==========================================================================
// TOTP (Time-based One-Time Password) Functions
// ==========================================================================

const TOTP_ISSUER: &str = "Kubarr";

/// Generate a new TOTP secret (base32 encoded)
pub fn generate_totp_secret() -> String {
    use totp_rs::Secret;
    Secret::generate_secret().to_encoded().to_string()
}

/// Create a TOTP instance for verification
fn create_totp(secret: &str, account_name: &str) -> Result<totp_rs::TOTP> {
    use totp_rs::{Algorithm, Secret, TOTP};

    let secret_bytes = Secret::Encoded(secret.to_string())
        .to_bytes()
        .map_err(|e| AppError::Internal(format!("Invalid TOTP secret: {}", e)))?;

    TOTP::new(
        Algorithm::SHA1,
        6,  // digits
        1,  // skew (allow 1 step before/after for clock drift)
        30, // step (30 seconds)
        secret_bytes,
        Some(TOTP_ISSUER.to_string()),
        account_name.to_string(),
    )
    .map_err(|e| AppError::Internal(format!("Failed to create TOTP: {}", e)))
}

/// Verify a TOTP code
pub fn verify_totp(secret: &str, code: &str, account_name: &str) -> Result<bool> {
    let totp = create_totp(secret, account_name)?;
    Ok(totp.check_current(code).unwrap_or(false))
}

/// Get TOTP provisioning URI for QR code generation
pub fn get_totp_provisioning_uri(secret: &str, account_name: &str) -> Result<String> {
    let totp = create_totp(secret, account_name)?;
    Ok(totp.get_url())
}

/// Get TOTP QR code as base64-encoded PNG data URL
#[allow(dead_code)]
pub fn get_totp_qr_code_base64(secret: &str, account_name: &str) -> Result<String> {
    let totp = create_totp(secret, account_name)?;
    let base64 = totp
        .get_qr_base64()
        .map_err(|e| AppError::Internal(format!("Failed to generate QR code: {}", e)))?;
    // Return as data URL for direct use in <img src="">
    Ok(format!("data:image/png;base64,{}", base64))
}

/// Generate a 2FA challenge token
pub fn generate_2fa_challenge_token() -> String {
    generate_random_string(32)
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
