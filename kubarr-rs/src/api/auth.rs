use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::api::extractors::AuthUser;
use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::services::{
    get_jwks, verify_password, OAuth2Service,
};
use crate::state::AppState;

/// Create auth routes
pub fn auth_routes(state: AppState) -> Router {
    Router::new()
        // Login endpoints - redirect to frontend, handle form POST
        .route("/login", get(login_page).post(login_submit))
        // OAuth2/OIDC endpoints
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/introspect", post(introspect))
        .route("/userinfo", get(userinfo))
        .route("/revoke", post(revoke))
        .route("/jwks", get(jwks))
        .route("/.well-known/openid-configuration", get(openid_configuration))
        // Direct API login
        .route("/api/login", post(api_login))
        // Admin endpoints
        .route("/admin/regenerate-client-secret", post(regenerate_client_secret))
        .with_state(state)
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LoginPageQuery {
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub code_challenge: String,
    #[serde(default = "default_code_challenge_method")]
    pub code_challenge_method: String,
    pub error: Option<String>,
}

fn default_code_challenge_method() -> String {
    "S256".to_string()
}

#[derive(Debug, Deserialize)]
pub struct LoginFormData {
    pub username: String,
    pub password: String,
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub code_challenge: String,
    #[serde(default = "default_code_challenge_method")]
    pub code_challenge_method: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
    pub is_active: bool,
    pub is_approved: bool,
}

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
pub struct IntrospectRequest {
    pub token: String,
    pub client_id: String,
    pub client_secret: Option<String>,
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
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(basic_creds.trim()) {
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

/// Login page - serves HTML login form directly
async fn login_page(
    Query(params): Query<LoginPageQuery>,
) -> Response {
    let error_html = params.error.as_ref().map(|err| format!(
        r#"<div class="rounded-md bg-red-900 p-4 mb-4">
            <div class="text-sm text-red-200">{}</div>
        </div>"#,
        html_escape(err)
    )).unwrap_or_default();

    let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Kubarr - Login</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="min-h-screen bg-gray-900 flex items-center justify-center px-4">
    <div class="max-w-md w-full space-y-8">
        <div>
            <h2 class="mt-6 text-center text-3xl font-extrabold text-white">
                Kubarr Dashboard
            </h2>
            <p class="mt-2 text-center text-sm text-gray-400">
                Sign in to your account
            </p>
        </div>
        <form class="mt-8 space-y-6" method="POST" action="/auth/login">
            {}
            <div class="rounded-md shadow-sm -space-y-px">
                <div>
                    <label for="username" class="sr-only">Username</label>
                    <input id="username" name="username" type="text" required autofocus
                        class="appearance-none rounded-none relative block w-full px-3 py-2 border border-gray-700 placeholder-gray-500 text-white bg-gray-800 rounded-t-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                        placeholder="Username" />
                </div>
                <div>
                    <label for="password" class="sr-only">Password</label>
                    <input id="password" name="password" type="password" required
                        class="appearance-none rounded-none relative block w-full px-3 py-2 border border-gray-700 placeholder-gray-500 text-white bg-gray-800 rounded-b-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                        placeholder="Password" />
                </div>
            </div>
            <input type="hidden" name="client_id" value="{}">
            <input type="hidden" name="redirect_uri" value="{}">
            <input type="hidden" name="scope" value="{}">
            <input type="hidden" name="state" value="{}">
            <input type="hidden" name="code_challenge" value="{}">
            <input type="hidden" name="code_challenge_method" value="{}">
            <div>
                <button type="submit"
                    class="group relative w-full flex justify-center py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500">
                    Sign in
                </button>
            </div>
        </form>
    </div>
</body>
</html>"#,
        error_html,
        html_escape(&params.client_id),
        html_escape(&params.redirect_uri),
        html_escape(&params.scope),
        html_escape(&params.state),
        html_escape(&params.code_challenge),
        html_escape(&params.code_challenge_method),
    );

    Html(html).into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Login form submission - validates credentials and returns auth code redirect
async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginFormData>,
) -> Result<Response> {
    use crate::db::User;

    // Find user
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&form.username)
        .fetch_optional(&state.pool)
        .await?;

    let user = match user {
        Some(u) => u,
        None => {
            // Redirect back to login with error
            let error_url = format!(
                "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
                urlencoding::encode(&form.client_id),
                urlencoding::encode(&form.redirect_uri),
                urlencoding::encode(&form.scope),
                urlencoding::encode(&form.state),
                urlencoding::encode(&form.code_challenge),
                urlencoding::encode(&form.code_challenge_method),
                urlencoding::encode("Invalid username or password")
            );
            return Ok(Redirect::to(&error_url).into_response());
        }
    };

    // Verify password
    if !verify_password(&form.password, &user.hashed_password) {
        let error_url = format!(
            "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Invalid username or password")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Check if user is active
    if !user.is_active {
        let error_url = format!(
            "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Account is inactive")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Check if user is approved
    if !user.is_approved {
        let error_url = format!(
            "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Account pending approval")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Create authorization code
    let oauth2_service = OAuth2Service::new(&state.pool);
    let code = oauth2_service
        .create_authorization_code(
            &form.client_id,
            user.id,
            &form.redirect_uri,
            Some(&form.scope),
            Some(&form.code_challenge),
            Some(&form.code_challenge_method),
            600, // 10 minutes expiry
        )
        .await?;

    // Redirect to callback with authorization code
    let callback_url = if form.redirect_uri.contains('?') {
        format!("{}&code={}&state={}", form.redirect_uri, code, form.state)
    } else {
        format!("{}?code={}&state={}", form.redirect_uri, code, form.state)
    };

    Ok(Redirect::to(&callback_url).into_response())
}

/// Direct API login endpoint for JSON requests
async fn api_login(
    State(state): State<AppState>,
    Json(login): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    use crate::db::User;
    use crate::services::create_access_token;

    // Find user
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&login.username)
        .fetch_optional(&state.pool)
        .await?;

    let user = match user {
        Some(u) => u,
        None => {
            return Err(AppError::Unauthorized(
                "Invalid username or password".to_string(),
            ))
        }
    };

    // Verify password
    if !verify_password(&login.password, &user.hashed_password) {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Check if user is active
    if !user.is_active {
        return Err(AppError::Forbidden("Account is inactive".to_string()));
    }

    // Check if user is approved
    if !user.is_approved {
        return Err(AppError::Forbidden("Account pending approval".to_string()));
    }

    // Create JWT token
    let access_token = create_access_token(&user.id.to_string(), Some(&user.email), None, None, None)?;

    Ok(Json(LoginResponse {
        access_token,
        token_type: "bearer".to_string(),
        user: UserInfo {
            id: user.id,
            username: user.username,
            email: user.email,
            is_admin: user.is_admin,
            is_active: user.is_active,
            is_approved: user.is_approved,
        },
    }))
}

/// OAuth2 authorization endpoint
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
    let oauth2_service = OAuth2Service::new(&state.pool);
    let client = oauth2_service.get_client(&params.client_id).await?;

    if client.is_none() {
        return Err(AppError::BadRequest("Invalid client_id".to_string()));
    }

    // Build login URL with parameters (URL-encoded to preserve special chars in state/redirect_uri)
    let login_url = format!(
        "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}",
        urlencoding::encode(&params.client_id),
        urlencoding::encode(&params.redirect_uri),
        urlencoding::encode(&params.scope.unwrap_or_default()),
        urlencoding::encode(&params.state.unwrap_or_default()),
        urlencoding::encode(&params.code_challenge.unwrap_or_default()),
        urlencoding::encode(&params.code_challenge_method.unwrap_or_else(|| "S256".to_string()))
    );

    Ok(Redirect::to(&login_url).into_response())
}

/// OAuth2 token endpoint
async fn token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(params): Form<TokenRequest>,
) -> Result<Json<TokenResponse>> {
    let oauth2_service = OAuth2Service::new(&state.pool);

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
                params.code_verifier.as_ref().map(|v| &v[..std::cmp::min(8, v.len())])
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
                    tracing::warn!("Authorization code validation failed for code: {}", &code[..std::cmp::min(16, code.len())]);
                    return Err(AppError::BadRequest(
                        "Invalid authorization code".to_string(),
                    ))
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
                None => {
                    return Err(AppError::BadRequest("Invalid refresh token".to_string()))
                }
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

/// OAuth2 token introspection endpoint
async fn introspect(
    State(state): State<AppState>,
    Json(request): Json<IntrospectRequest>,
) -> Result<impl IntoResponse> {
    let oauth2_service = OAuth2Service::new(&state.pool);

    // Validate client if credentials provided
    if let Some(ref secret) = request.client_secret {
        if !oauth2_service
            .validate_client(&request.client_id, Some(secret))
            .await?
        {
            return Err(AppError::Unauthorized(
                "Invalid client credentials".to_string(),
            ));
        }
    }

    let result = oauth2_service.introspect_token(&request.token).await?;
    Ok(Json(result))
}

/// OIDC UserInfo endpoint
async fn userinfo(AuthUser(user): AuthUser) -> Json<serde_json::Value> {
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
    let oauth2_service = OAuth2Service::new(&state.pool);

    // Validate client if credentials provided
    if let (Some(ref client_id), Some(ref secret)) = (&request.client_id, &request.client_secret) {
        if !oauth2_service.validate_client(client_id, Some(secret)).await? {
            return Err(AppError::Unauthorized(
                "Invalid client credentials".to_string(),
            ));
        }
    }

    oauth2_service.revoke_token(&request.token).await?;

    Ok(Json(serde_json::json!({"message": "Token revoked"})))
}

/// JSON Web Key Set endpoint
async fn jwks() -> Result<Json<serde_json::Value>> {
    let jwks = get_jwks()?;
    Ok(Json(jwks))
}

/// OIDC Discovery endpoint
async fn openid_configuration() -> Json<serde_json::Value> {
    let base_url = &CONFIG.oauth2_issuer_url;
    let issuer = format!("{}/auth", base_url);

    Json(serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize", issuer),
        "token_endpoint": format!("{}/token", issuer),
        "userinfo_endpoint": format!("{}/userinfo", issuer),
        "introspection_endpoint": format!("{}/introspect", issuer),
        "revocation_endpoint": format!("{}/revoke", issuer),
        "jwks_uri": format!("{}/jwks", issuer),
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
    AuthUser(user): AuthUser,
) -> Result<Json<serde_json::Value>> {
    use crate::services::{generate_random_string, hash_client_secret};

    // Check admin
    if !user.is_admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    // Find oauth2-proxy client
    let oauth2_service = OAuth2Service::new(&state.pool);
    let client = oauth2_service.get_client("oauth2-proxy").await?;

    if client.is_none() {
        return Err(AppError::NotFound(
            "oauth2-proxy client not found".to_string(),
        ));
    }

    // Generate new secret
    let new_secret = generate_random_string(32);
    let secret_hash = hash_client_secret(&new_secret)?;

    // Update client
    sqlx::query("UPDATE oauth2_clients SET client_secret_hash = ? WHERE client_id = 'oauth2-proxy'")
        .bind(&secret_hash)
        .execute(&state.pool)
        .await?;

    // Store the plain secret in SystemSettings
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, description, updated_at)
        VALUES ('oauth2_client_secret', ?, 'OAuth2-proxy client secret (for syncing to Kubernetes)', datetime('now'))
        ON CONFLICT(key) DO UPDATE SET value = ?, updated_at = datetime('now')
        "#,
    )
    .bind(&new_secret)
    .bind(&new_secret)
    .execute(&state.pool)
    .await?;

    // TODO: Sync to Kubernetes secret

    Ok(Json(serde_json::json!({
        "client_id": "oauth2-proxy",
        "client_secret": new_secret,
        "synced_to_kubernetes": false,
        "message": "Client secret regenerated. Kubernetes sync not yet implemented in Rust backend.",
    })))
}
