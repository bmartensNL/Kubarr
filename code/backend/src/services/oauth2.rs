use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::api::extractors::{get_user_app_access, get_user_permissions};
use crate::db::entities::prelude::*;
use crate::db::entities::{oauth2_authorization_code, oauth2_client, oauth2_token};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{create_test_db_with_seed, create_test_user_with_role};

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
            create_test_user_with_role(&db, "tokenuser", "token@test.com", "password", "admin")
                .await;

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
            create_test_user_with_role(&db, "accessuser", "access@test.com", "password", "admin")
                .await;

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
            create_test_user_with_role(&db, "revokeuser", "revoke@test.com", "password", "admin")
                .await;

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
            create_test_user_with_role(&db, "introuser", "intro@test.com", "password", "admin")
                .await;

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
}
