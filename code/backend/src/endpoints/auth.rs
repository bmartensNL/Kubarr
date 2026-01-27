use axum::{
    extract::{Extension, Query, State},
    response::Response,
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use crate::endpoints::extractors::AdminUser;
use crate::middleware::AuthenticatedUser;
use crate::config::CONFIG;
use crate::models::prelude::*;
use crate::models::{oauth2_client, system_setting};
use crate::error::{AppError, Result};
use crate::services::OAuth2Service;
use crate::state::AppState;

/// Create auth routes (OAuth2/OIDC provider endpoints)
pub fn auth_routes(state: AppState) -> Router {
    Router::new()
        // OAuth2/OIDC endpoints
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/userinfo", get(userinfo))
        .route("/revoke", post(revoke))
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        // Admin endpoints
        .route(
            "/admin/regenerate-client-secret",
            post(regenerate_client_secret),
        )
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeRequest {
    pub token: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract client credentials from Basic auth header or form body
fn extract_client_credentials(
    headers: &axum::http::HeaderMap,
    params: &TokenRequest,
) -> Result<(String, Option<String>)> {
    // Try Basic auth header first
    if let Some(auth_header) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(basic_creds) = auth_str.strip_prefix("Basic ") {
                if let Ok(decoded) =
                    base64::engine::general_purpose::STANDARD.decode(basic_creds.trim())
                {
                    if let Ok(creds_str) = String::from_utf8(decoded) {
                        if let Some((id, secret)) = creds_str.split_once(':') {
                            return Ok((id.to_string(), Some(secret.to_string())));
                        }
                    }
                }
            }
        }
    }

    // Fall back to form body
    let client_id = params
        .client_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("client_id is required".to_string()))?;

    Ok((client_id, params.client_secret.clone()))
}

// ============================================================================
// Endpoint Handlers
// ============================================================================

/// OAuth2 authorization endpoint
/// Returns unauthorized if no valid session - oauth2-proxy handles the login flow
async fn authorize(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeQuery>,
) -> Result<Response> {
    // Validate response_type
    if params.response_type != "code" {
        return Err(AppError::BadRequest(
            "Only 'code' response_type is supported".to_string(),
        ));
    }

    // Validate client
    let oauth2_service = OAuth2Service::new(&state.db);
    let client = oauth2_service.get_client(&params.client_id).await?;

    if client.is_none() {
        return Err(AppError::BadRequest("Invalid client_id".to_string()));
    }

    // This endpoint requires authentication via oauth2-proxy
    // Return unauthorized - oauth2-proxy will handle the login flow
    Err(AppError::Unauthorized("Login required".to_string()))
}

/// OAuth2 token endpoint
async fn token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(params): Form<TokenRequest>,
) -> Result<Json<TokenResponse>> {
    let oauth2_service = OAuth2Service::new(&state.db);

    // Extract client credentials from Basic auth header or form body
    let (client_id, client_secret) = extract_client_credentials(&headers, &params)?;

    // Validate client
    if !oauth2_service
        .validate_client(&client_id, client_secret.as_deref())
        .await?
    {
        return Err(AppError::Unauthorized(
            "Invalid client credentials".to_string(),
        ));
    }

    match params.grant_type.as_str() {
        "authorization_code" => {
            let code = params
                .code
                .ok_or_else(|| AppError::BadRequest("code is required".to_string()))?;
            let redirect_uri = params
                .redirect_uri
                .ok_or_else(|| AppError::BadRequest("redirect_uri is required".to_string()))?;

            tracing::info!(
                "Token exchange: code={}, client_id={}, redirect_uri={}, code_verifier={:?}",
                &code[..std::cmp::min(16, code.len())],
                &client_id,
                &redirect_uri,
                params
                    .code_verifier
                    .as_ref()
                    .map(|v| &v[..std::cmp::min(8, v.len())])
            );

            // Validate authorization code
            let auth_code = oauth2_service
                .validate_authorization_code(
                    &code,
                    &client_id,
                    &redirect_uri,
                    params.code_verifier.as_deref(),
                )
                .await?;

            let auth_code = match auth_code {
                Some(ac) => ac,
                None => {
                    tracing::warn!(
                        "Authorization code validation failed for code: {}",
                        &code[..std::cmp::min(16, code.len())]
                    );
                    return Err(AppError::BadRequest(
                        "Invalid authorization code".to_string(),
                    ));
                }
            };

            // Create tokens
            let tokens = oauth2_service
                .create_tokens(
                    &client_id,
                    auth_code.user_id,
                    auth_code.scope.as_deref(),
                    3600,   // 1 hour
                    604800, // 7 days
                )
                .await?;

            Ok(Json(TokenResponse {
                access_token: tokens.access_token.clone(),
                token_type: "Bearer".to_string(),
                expires_in: tokens.expires_in,
                refresh_token: Some(tokens.refresh_token),
                id_token: Some(tokens.access_token), // Same as access token for now
                scope: tokens.scope,
            }))
        }
        "refresh_token" => {
            let refresh_token = params
                .refresh_token
                .ok_or_else(|| AppError::BadRequest("refresh_token is required".to_string()))?;

            let tokens = oauth2_service
                .refresh_access_token(&refresh_token, &client_id)
                .await?;

            let tokens = match tokens {
                Some(t) => t,
                None => return Err(AppError::BadRequest("Invalid refresh token".to_string())),
            };

            Ok(Json(TokenResponse {
                access_token: tokens.access_token,
                token_type: "Bearer".to_string(),
                expires_in: tokens.expires_in,
                refresh_token: Some(tokens.refresh_token),
                id_token: None,
                scope: tokens.scope,
            }))
        }
        _ => Err(AppError::BadRequest(format!(
            "Unsupported grant_type: {}",
            params.grant_type
        ))),
    }
}

/// OIDC UserInfo endpoint
async fn userinfo(
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Json<serde_json::Value> {
    let user = auth_user.0;
    Json(serde_json::json!({
        "sub": user.id.to_string(),
        "name": user.username,
        "preferred_username": user.username,
        "email": user.email,
        "email_verified": user.is_approved,
    }))
}

/// OAuth2 token revocation endpoint
async fn revoke(
    State(state): State<AppState>,
    Json(request): Json<RevokeRequest>,
) -> Result<Json<serde_json::Value>> {
    let oauth2_service = OAuth2Service::new(&state.db);

    // Validate client if credentials provided
    if let (Some(ref client_id), Some(ref secret)) = (&request.client_id, &request.client_secret) {
        if !oauth2_service
            .validate_client(client_id, Some(secret))
            .await?
        {
            return Err(AppError::Unauthorized(
                "Invalid client credentials".to_string(),
            ));
        }
    }

    oauth2_service.revoke_token(&request.token).await?;

    Ok(Json(serde_json::json!({"message": "Token revoked"})))
}

/// OIDC Discovery endpoint
async fn openid_configuration() -> Json<serde_json::Value> {
    let issuer = CONFIG.oauth2_issuer_url.clone();

    Json(serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize", issuer),
        "token_endpoint": format!("{}/token", issuer),
        "userinfo_endpoint": format!("{}/userinfo", issuer),
        "revocation_endpoint": format!("{}/revoke", issuer),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post", "client_secret_basic"],
        "code_challenge_methods_supported": ["S256", "plain"],
        "scopes_supported": ["openid", "profile", "email"],
    }))
}

/// Regenerate oauth2-proxy client secret (admin only)
async fn regenerate_client_secret(
    State(state): State<AppState>,
    AdminUser(_user): AdminUser,
) -> Result<Json<serde_json::Value>> {
    use crate::services::{generate_random_string, hash_client_secret};

    // Find oauth2-proxy client
    let oauth2_service = OAuth2Service::new(&state.db);
    let client = oauth2_service.get_client("oauth2-proxy").await?;

    let client = match client {
        Some(c) => c,
        None => {
            return Err(AppError::NotFound(
                "oauth2-proxy client not found".to_string(),
            ));
        }
    };

    // Generate new secret
    let new_secret = generate_random_string(32);
    let secret_hash = hash_client_secret(&new_secret)?;

    // Update client
    let mut client_model: oauth2_client::ActiveModel = client.into();
    client_model.client_secret_hash = Set(secret_hash);
    client_model.update(&state.db).await?;

    // Store the plain secret in SystemSettings (upsert)
    let now = Utc::now();
    let existing = SystemSetting::find_by_id("oauth2_client_secret")
        .one(&state.db)
        .await?;

    if let Some(setting) = existing {
        let mut setting_model: system_setting::ActiveModel = setting.into();
        setting_model.value = Set(new_secret.clone());
        setting_model.updated_at = Set(now);
        setting_model.update(&state.db).await?;
    } else {
        let new_setting = system_setting::ActiveModel {
            key: Set("oauth2_client_secret".to_string()),
            value: Set(new_secret.clone()),
            description: Set(Some(
                "OAuth2-proxy client secret (for syncing to Kubernetes)".to_string(),
            )),
            updated_at: Set(now),
        };
        new_setting.insert(&state.db).await?;
    }

    // TODO: Sync to Kubernetes secret

    Ok(Json(serde_json::json!({
        "client_id": "oauth2-proxy",
        "client_secret": new_secret,
        "synced_to_kubernetes": false,
        "message": "Client secret regenerated. Kubernetes sync not yet implemented in Rust backend.",
    })))
}

