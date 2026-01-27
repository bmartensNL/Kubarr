use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::api::extractors::{get_user_app_access, get_user_permissions};
use crate::models::prelude::*;
use crate::models::{oauth2_authorization_code, oauth2_client, oauth2_token};
use crate::error::{AppError, Result};
use crate::services::security::{
    create_access_token, create_refresh_token, generate_authorization_code, verify_client_secret,
    verify_pkce,
};

/// OAuth2 service for handling authorization and token operations
pub struct OAuth2Service<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> OAuth2Service<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    /// Get OAuth2 client by ID
    pub async fn get_client(&self, client_id: &str) -> Result<Option<oauth2_client::Model>> {
        let client = OAuth2Client::find_by_id(client_id).one(self.db).await?;
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

        let auth_code = oauth2_authorization_code::ActiveModel {
            code: Set(code.clone()),
            client_id: Set(client_id.to_string()),
            user_id: Set(user_id),
            redirect_uri: Set(redirect_uri.to_string()),
            scope: Set(scope.map(String::from)),
            code_challenge: Set(code_challenge.map(String::from)),
            code_challenge_method: Set(code_challenge_method.map(String::from)),
            expires_at: Set(expires_at),
            used: Set(false),
            created_at: Set(now),
        };

        auth_code.insert(self.db).await?;

        Ok(code)
    }

    /// Validate and consume an authorization code
    pub async fn validate_authorization_code(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<Option<oauth2_authorization_code::Model>> {
        let auth_code = OAuth2AuthorizationCode::find_by_id(code)
            .one(self.db)
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
            tracing::warn!(
                "Client ID mismatch: stored={}, provided={}",
                auth_code.client_id,
                client_id
            );
            return Ok(None);
        }

        // Check redirect URI
        if auth_code.redirect_uri != redirect_uri {
            tracing::warn!(
                "Redirect URI mismatch: stored={}, provided={}",
                auth_code.redirect_uri,
                redirect_uri
            );
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
        let mut auth_code_model: oauth2_authorization_code::ActiveModel = auth_code.clone().into();
        auth_code_model.used = Set(true);
        auth_code_model.update(self.db).await?;

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
        let found_user = User::find_by_id(user_id)
            .one(self.db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        // Fetch user permissions and allowed apps for JWT embedding
        let permissions = get_user_permissions(self.db, user_id).await;
        let allowed_apps = get_user_app_access(self.db, user_id).await;

        // Create access token with embedded permissions
        let access_token = create_access_token(
            &user_id.to_string(),
            Some(&found_user.email),
            scope,
            Some(client_id),
            Some(access_token_expires_in),
            Some(permissions),
            Some(allowed_apps),
        )?;

        // Create refresh token
        let refresh_token = create_refresh_token(
            &user_id.to_string(),
            Some(&found_user.email),
            scope,
            Some(client_id),
            Some(refresh_token_expires_in),
        )?;

        let now = Utc::now();
        let expires_at = now + Duration::seconds(access_token_expires_in);
        let refresh_expires_at = now + Duration::seconds(refresh_token_expires_in);

        // Store in database
        let token_model = oauth2_token::ActiveModel {
            access_token: Set(access_token.clone()),
            refresh_token: Set(Some(refresh_token.clone())),
            client_id: Set(client_id.to_string()),
            user_id: Set(user_id),
            scope: Set(scope.map(String::from)),
            expires_at: Set(expires_at),
            refresh_expires_at: Set(Some(refresh_expires_at)),
            revoked: Set(false),
            created_at: Set(now),
            ..Default::default()
        };

        token_model.insert(self.db).await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: access_token_expires_in,
            refresh_expires_in: refresh_token_expires_in,
            scope: scope.map(String::from),
        })
    }

    /// Validate an access token
    pub async fn validate_access_token(
        &self,
        access_token: &str,
    ) -> Result<Option<oauth2_token::Model>> {
        let token = OAuth2Token::find()
            .filter(oauth2_token::Column::AccessToken.eq(access_token))
            .one(self.db)
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
        let token = OAuth2Token::find()
            .filter(oauth2_token::Column::RefreshToken.eq(refresh_token))
            .one(self.db)
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
        let mut token_model: oauth2_token::ActiveModel = token.clone().into();
        token_model.revoked = Set(true);
        token_model.update(self.db).await?;

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
        let found = OAuth2Token::find()
            .filter(oauth2_token::Column::AccessToken.eq(token))
            .one(self.db)
            .await?;

        if let Some(t) = found {
            let mut token_model: oauth2_token::ActiveModel = t.into();
            token_model.revoked = Set(true);
            token_model.update(self.db).await?;
            return Ok(true);
        }

        // Try as refresh token
        let found = OAuth2Token::find()
            .filter(oauth2_token::Column::RefreshToken.eq(token))
            .one(self.db)
            .await?;

        if let Some(t) = found {
            let mut token_model: oauth2_token::ActiveModel = t.into();
            token_model.revoked = Set(true);
            token_model.update(self.db).await?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Introspect a token
    pub async fn introspect_token(&self, token: &str) -> Result<TokenIntrospection> {
        let token_record = match self.validate_access_token(token).await? {
            Some(t) => t,
            None => return Ok(TokenIntrospection::inactive()),
        };

        // Get user
        let found_user = User::find_by_id(token_record.user_id).one(self.db).await?;

        let found_user = match found_user {
            Some(u) if u.is_active && u.is_approved => u,
            _ => return Ok(TokenIntrospection::inactive()),
        };

        Ok(TokenIntrospection {
            active: true,
            sub: Some(found_user.id.to_string()),
            username: Some(found_user.username),
            email: Some(found_user.email),
            scope: token_record.scope,
            exp: Some(token_record.expires_at.timestamp()),
            client_id: Some(token_record.client_id),
        })
    }

    /// Create an OAuth2 client
    #[allow(dead_code)]
    pub async fn create_client(
        &self,
        client_id: &str,
        client_secret: &str,
        name: &str,
        redirect_uris: &[String],
    ) -> Result<oauth2_client::Model> {
        use crate::services::security::hash_client_secret;

        let secret_hash = hash_client_secret(client_secret)?;
        let redirect_uris_json = serde_json::to_string(redirect_uris)?;
        let now = Utc::now();

        let client_model = oauth2_client::ActiveModel {
            client_id: Set(client_id.to_string()),
            client_secret_hash: Set(secret_hash),
            name: Set(name.to_string()),
            redirect_uris: Set(redirect_uris_json),
            created_at: Set(now),
        };

        let created = client_model.insert(self.db).await?;

        Ok(created)
    }
}

/// Token pair returned after successful token creation
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    #[allow(dead_code)]
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
