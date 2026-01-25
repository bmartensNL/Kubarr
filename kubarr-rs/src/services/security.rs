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
    pub sub: String,           // Subject (user identifier)
    pub iss: String,           // Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,   // Audience (client_id)
    pub email: Option<String>, // User email
    pub exp: i64,              // Expiration time
    pub iat: i64,              // Issued at
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>, // "refresh" for refresh tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>, // OAuth2 scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>, // OAuth2 client ID
}

/// Get the JWT private key (PEM format)
pub fn get_private_key() -> Result<String> {
    // Check cache first
    {
        let cache = PRIVATE_KEY.read();
        if let Some(key) = cache.as_ref() {
            return Ok(key.clone());
        }
    }

    // Try to load from file
    if CONFIG.jwt_private_key_path.exists() {
        let content = fs::read_to_string(&CONFIG.jwt_private_key_path)
            .map_err(|e| AppError::Internal(format!("Failed to read private key: {}", e)))?;

        if !content.trim().is_empty() {
            let mut cache = PRIVATE_KEY.write();
            *cache = Some(content.clone());
            return Ok(content);
        }
    }

    // Generate in-memory key for development
    tracing::warn!("JWT private key not found, generating temporary key");
    let (private_pem, public_pem) = generate_rsa_key_pair()?;

    {
        let mut priv_cache = PRIVATE_KEY.write();
        *priv_cache = Some(private_pem.clone());
    }
    {
        let mut pub_cache = PUBLIC_KEY.write();
        *pub_cache = Some(public_pem);
    }

    Ok(private_pem)
}

/// Get the JWT public key (PEM format)
pub fn get_public_key() -> Result<String> {
    // Check cache first
    {
        let cache = PUBLIC_KEY.read();
        if let Some(key) = cache.as_ref() {
            return Ok(key.clone());
        }
    }

    // Try to load from file
    if CONFIG.jwt_public_key_path.exists() {
        let content = fs::read_to_string(&CONFIG.jwt_public_key_path)
            .map_err(|e| AppError::Internal(format!("Failed to read public key: {}", e)))?;

        if !content.trim().is_empty() {
            let mut cache = PUBLIC_KEY.write();
            *cache = Some(content.clone());
            return Ok(content);
        }
    }

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
        token_type: None,
        scope: scope.map(String::from),
        client_id: client_id.map(String::from),
    };

    let private_key = get_private_key()?;
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid private key: {}", e)))?;

    let header = Header::new(jsonwebtoken::Algorithm::RS256);
    encode(&header, &claims, &encoding_key).map_err(|e| e.into())
}

/// Create a JWT refresh token
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
        token_type: Some("refresh".to_string()),
        scope: scope.map(String::from),
        client_id: client_id.map(String::from),
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
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()";
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

    #[test]
    fn test_password_hashing() {
        let password = "test_password123";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_random_string() {
        let s1 = generate_random_string(16);
        let s2 = generate_random_string(16);
        assert_eq!(s1.len(), 32); // hex encoding doubles length
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_pkce_s256() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // This is the base64url-encoded SHA256 hash of the verifier
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(verify_pkce(verifier, challenge, "S256"));
    }

    #[test]
    fn test_pkce_plain() {
        let verifier = "test_verifier";
        assert!(verify_pkce(verifier, verifier, "plain"));
        assert!(!verify_pkce(verifier, "different", "plain"));
    }
}
