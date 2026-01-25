use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use crate::db::{OAuth2AuthorizationCode, OAuth2Client, OAuth2Token, User};
use crate::error::{AppError, Result};
use crate::services::security::{
    create_access_token, create_refresh_token, generate_authorization_code,
    verify_client_secret, verify_pkce,
};

/// OAuth2 service for handling authorization and token operations
pub struct OAuth2Service<'a> {
    pool: &'a SqlitePool,
}

impl<'a> OAuth2Service<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get OAuth2 client by ID
    pub async fn get_client(&self, client_id: &str) -> Result<Option<OAuth2Client>> {
        let client: Option<OAuth2Client> =
            sqlx::query_as("SELECT * FROM oauth2_clients WHERE client_id = ?")
                .bind(client_id)
                .fetch_optional(self.pool)
                .await?;
        Ok(client)
    }

    /// Validate OAuth2 client credentials
    pub async fn validate_client(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
    ) -> Result<bool> {
        let client = match self.get_client(client_id).await? {
            Some(c) => c,
            None => return Ok(false),
        };

        if let Some(secret) = client_secret {
            Ok(verify_client_secret(secret, &client.client_secret_hash))
        } else {
            // Public client (no secret required)
            Ok(true)
        }
    }

    /// Create an authorization code
    pub async fn create_authorization_code(
        &self,
        client_id: &str,
        user_id: i64,
        redirect_uri: &str,
        scope: Option<&str>,
        code_challenge: Option<&str>,
        code_challenge_method: Option<&str>,
        expires_in_secs: i64,
    ) -> Result<String> {
        let code = generate_authorization_code();
        let now = Utc::now();
        let expires_at = now + Duration::seconds(expires_in_secs);

        sqlx::query(
            r#"
            INSERT INTO oauth2_authorization_codes
            (code, client_id, user_id, redirect_uri, scope, code_challenge, code_challenge_method, expires_at, used, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
            "#,
        )
        .bind(&code)
        .bind(client_id)
        .bind(user_id)
        .bind(redirect_uri)
        .bind(scope)
        .bind(code_challenge)
        .bind(code_challenge_method)
        .bind(expires_at)
        .bind(now)
        .execute(self.pool)
        .await?;

        Ok(code)
    }

    /// Validate and consume an authorization code
    pub async fn validate_authorization_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<Option<OAuth2AuthorizationCode>> {
        let auth_code: Option<OAuth2AuthorizationCode> =
            sqlx::query_as("SELECT * FROM oauth2_authorization_codes WHERE code = ?")
                .bind(code)
                .fetch_optional(self.pool)
                .await?;

        let auth_code = match auth_code {
            Some(ac) => ac,
            None => {
                tracing::warn!("Code not found in database");
                return Ok(None);
            }
        };

        tracing::info!(
            "Found code: client_id={}, redirect_uri={}, used={}, expires_at={}, code_challenge={:?}",
            auth_code.client_id,
            auth_code.redirect_uri,
            auth_code.used,
            auth_code.expires_at,
            auth_code.code_challenge.as_ref().map(|c| &c[..std::cmp::min(8, c.len())])
        );

        // Check if already used
        if auth_code.used {
            tracing::warn!("Code already used");
            return Ok(None);
        }

        // Check expiration
        if Utc::now() > auth_code.expires_at {
            tracing::warn!("Code expired");
            return Ok(None);
        }

        // Check client ID
        if auth_code.client_id != client_id {
            tracing::warn!("Client ID mismatch: stored={}, provided={}", auth_code.client_id, client_id);
            return Ok(None);
        }

        // Check redirect URI
        if auth_code.redirect_uri != redirect_uri {
            tracing::warn!("Redirect URI mismatch: stored={}, provided={}", auth_code.redirect_uri, redirect_uri);
            return Ok(None);
        }

        // Verify PKCE if present
        if let Some(challenge) = &auth_code.code_challenge {
            if !challenge.is_empty() {
                let verifier = match code_verifier {
                    Some(v) => v,
                    None => {
                        tracing::warn!("PKCE challenge present but no verifier provided");
                        return Ok(None);
                    }
                };
                let method = auth_code.code_challenge_method.as_deref().unwrap_or("S256");
                if !verify_pkce(verifier, challenge, method) {
                    tracing::warn!("PKCE verification failed");
                    return Ok(None);
                }
            }
        }

        // Mark as used
        sqlx::query("UPDATE oauth2_authorization_codes SET used = 1 WHERE code = ?")
            .bind(code)
            .execute(self.pool)
            .await?;

        Ok(Some(auth_code))
    }

    /// Create access and refresh tokens
    pub async fn create_tokens(
        &self,
        client_id: &str,
        user_id: i64,
        scope: Option<&str>,
        access_token_expires_in: i64,
        refresh_token_expires_in: i64,
    ) -> Result<TokenPair> {
        // Get user info
        let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        // Create access token
        let access_token = create_access_token(
            &user_id.to_string(),
            Some(&user.email),
            scope,
            Some(client_id),
            Some(access_token_expires_in),
        )?;

        // Create refresh token
        let refresh_token = create_refresh_token(
            &user_id.to_string(),
            Some(&user.email),
            scope,
            Some(client_id),
            Some(refresh_token_expires_in),
        )?;

        let now = Utc::now();
        let expires_at = now + Duration::seconds(access_token_expires_in);
        let refresh_expires_at = now + Duration::seconds(refresh_token_expires_in);

        // Store in database
        sqlx::query(
            r#"
            INSERT INTO oauth2_tokens
            (access_token, refresh_token, client_id, user_id, scope, expires_at, refresh_expires_at, revoked, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)
            "#,
        )
        .bind(&access_token)
        .bind(&refresh_token)
        .bind(client_id)
        .bind(user_id)
        .bind(scope)
        .bind(expires_at)
        .bind(refresh_expires_at)
        .bind(now)
        .execute(self.pool)
        .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: access_token_expires_in,
            refresh_expires_in: refresh_token_expires_in,
            scope: scope.map(String::from),
        })
    }

    /// Validate an access token
    pub async fn validate_access_token(&self, access_token: &str) -> Result<Option<OAuth2Token>> {
        let token: Option<OAuth2Token> =
            sqlx::query_as("SELECT * FROM oauth2_tokens WHERE access_token = ?")
                .bind(access_token)
                .fetch_optional(self.pool)
                .await?;

        let token = match token {
            Some(t) => t,
            None => return Ok(None),
        };

        // Check if revoked
        if token.revoked {
            return Ok(None);
        }

        // Check expiration
        if Utc::now() > token.expires_at {
            return Ok(None);
        }

        Ok(Some(token))
    }

    /// Refresh an access token
    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<Option<TokenPair>> {
        let token: Option<OAuth2Token> =
            sqlx::query_as("SELECT * FROM oauth2_tokens WHERE refresh_token = ?")
                .bind(refresh_token)
                .fetch_optional(self.pool)
                .await?;

        let token = match token {
            Some(t) => t,
            None => return Ok(None),
        };

        // Check client ID
        if token.client_id != client_id {
            return Ok(None);
        }

        // Check if revoked
        if token.revoked {
            return Ok(None);
        }

        // Check refresh token expiration
        if let Some(ref refresh_exp) = token.refresh_expires_at {
            if Utc::now() > *refresh_exp {
                return Ok(None);
            }
        }

        // Revoke old tokens
        sqlx::query("UPDATE oauth2_tokens SET revoked = 1 WHERE id = ?")
            .bind(token.id)
            .execute(self.pool)
            .await?;

        // Create new tokens
        let new_tokens = self
            .create_tokens(
                client_id,
                token.user_id,
                token.scope.as_deref(),
                3600,   // 1 hour
                604800, // 7 days
            )
            .await?;

        Ok(Some(new_tokens))
    }

    /// Revoke a token
    pub async fn revoke_token(&self, token: &str) -> Result<bool> {
        // Try as access token
        let result = sqlx::query("UPDATE oauth2_tokens SET revoked = 1 WHERE access_token = ?")
            .bind(token)
            .execute(self.pool)
            .await?;

        if result.rows_affected() > 0 {
            return Ok(true);
        }

        // Try as refresh token
        let result = sqlx::query("UPDATE oauth2_tokens SET revoked = 1 WHERE refresh_token = ?")
            .bind(token)
            .execute(self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Introspect a token
    pub async fn introspect_token(&self, token: &str) -> Result<TokenIntrospection> {
        let token_record = match self.validate_access_token(token).await? {
            Some(t) => t,
            None => return Ok(TokenIntrospection::inactive()),
        };

        // Get user
        let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(token_record.user_id)
            .fetch_optional(self.pool)
            .await?;

        let user = match user {
            Some(u) if u.is_active && u.is_approved => u,
            _ => return Ok(TokenIntrospection::inactive()),
        };

        Ok(TokenIntrospection {
            active: true,
            sub: Some(user.id.to_string()),
            username: Some(user.username),
            email: Some(user.email),
            scope: token_record.scope,
            exp: Some(token_record.expires_at.timestamp()),
            client_id: Some(token_record.client_id),
        })
    }

    /// Create an OAuth2 client
    pub async fn create_client(
        &self,
        client_id: &str,
        client_secret: &str,
        name: &str,
        redirect_uris: &[String],
    ) -> Result<OAuth2Client> {
        use crate::services::security::hash_client_secret;

        let secret_hash = hash_client_secret(client_secret)?;
        let redirect_uris_json = serde_json::to_string(redirect_uris)?;

        sqlx::query(
            r#"
            INSERT INTO oauth2_clients (client_id, client_secret_hash, name, redirect_uris)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(client_id)
        .bind(&secret_hash)
        .bind(name)
        .bind(&redirect_uris_json)
        .execute(self.pool)
        .await?;

        let client: OAuth2Client =
            sqlx::query_as("SELECT * FROM oauth2_clients WHERE client_id = ?")
                .bind(client_id)
                .fetch_one(self.pool)
                .await?;

        Ok(client)
    }
}

/// Token pair returned after successful token creation
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub refresh_expires_in: i64,
    pub scope: Option<String>,
}

/// Token introspection response
#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenIntrospection {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

impl TokenIntrospection {
    pub fn inactive() -> Self {
        Self {
            active: false,
            sub: None,
            username: None,
            email: None,
            scope: None,
            exp: None,
            client_id: None,
        }
    }
}
