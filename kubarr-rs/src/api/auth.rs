use axum::{
    extract::{Query, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::api::extractors::{AdminUser, AuthUser};
use crate::config::CONFIG;
use crate::db::entities::{oauth2_client, system_setting, user};
use crate::db::entities::prelude::*;
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
        // Direct API login (returns token in body - legacy)
        .route("/api/login", post(api_login))
        // Session-based login (sets HttpOnly cookie)
        .route("/session/login", post(session_login))
        .route("/session/verify", get(session_verify))
        .route("/session/logout", post(session_logout))
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
    // Find user
    let found_user = User::find()
        .filter(user::Column::Username.eq(&form.username))
        .one(&state.db)
        .await?;

    let found_user = match found_user {
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
    if !verify_password(&form.password, &found_user.hashed_password) {
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
    if !found_user.is_active {
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
    if !found_user.is_approved {
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
    let oauth2_service = OAuth2Service::new(&state.db);
    let code = oauth2_service
        .create_authorization_code(
            &form.client_id,
            found_user.id,
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
    use crate::services::create_access_token;
    use crate::api::extractors::{get_user_permissions, get_user_app_access};

    // Find user
    let found_user = User::find()
        .filter(user::Column::Username.eq(&login.username))
        .one(&state.db)
        .await?;

    let found_user = match found_user {
        Some(u) => u,
        None => {
            return Err(AppError::Unauthorized(
                "Invalid username or password".to_string(),
            ))
        }
    };

    // Verify password
    if !verify_password(&login.password, &found_user.hashed_password) {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Check if user is active
    if !found_user.is_active {
        return Err(AppError::Forbidden("Account is inactive".to_string()));
    }

    // Check if user is approved
    if !found_user.is_approved {
        return Err(AppError::Forbidden("Account pending approval".to_string()));
    }

    // Fetch user permissions and allowed apps
    let permissions = get_user_permissions(&state.db, found_user.id).await;
    let allowed_apps = get_user_app_access(&state.db, found_user.id).await;

    // Create JWT token with embedded permissions
    let access_token = create_access_token(
        &found_user.id.to_string(),
        Some(&found_user.email),
        None,
        None,
        None,
        Some(permissions),
        Some(allowed_apps),
    )?;

    Ok(Json(LoginResponse {
        access_token,
        token_type: "bearer".to_string(),
        user: UserInfo {
            id: found_user.id,
            username: found_user.username,
            email: found_user.email,
            is_active: found_user.is_active,
            is_approved: found_user.is_approved,
        },
    }))
}

/// Session-based login - sets HttpOnly cookie
async fn session_login(
    State(state): State<AppState>,
    Json(login): Json<LoginRequest>,
) -> Result<Response> {
    use crate::services::create_access_token;
    use crate::api::extractors::{get_user_permissions, get_user_app_access};

    // Find user
    let found_user = User::find()
        .filter(user::Column::Username.eq(&login.username))
        .one(&state.db)
        .await?;

    let found_user = match found_user {
        Some(u) => u,
        None => {
            return Err(AppError::Unauthorized(
                "Invalid username or password".to_string(),
            ))
        }
    };

    // Verify password
    if !verify_password(&login.password, &found_user.hashed_password) {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Check if user is active
    if !found_user.is_active {
        return Err(AppError::Forbidden("Account is inactive".to_string()));
    }

    // Check if user is approved
    if !found_user.is_approved {
        return Err(AppError::Forbidden("Account pending approval".to_string()));
    }

    // Fetch user permissions and allowed apps
    let permissions = get_user_permissions(&state.db, found_user.id).await;
    let allowed_apps = get_user_app_access(&state.db, found_user.id).await;

    // Create JWT token with embedded permissions
    let access_token = create_access_token(
        &found_user.id.to_string(),
        Some(&found_user.email),
        None,
        None,
        None,
        Some(permissions),
        Some(allowed_apps),
    )?;

    // Set HttpOnly cookie with the token
    let cookie = format!(
        "kubarr_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800",
        access_token
    );

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    Ok((headers, Json(serde_json::json!({"success": true}))).into_response())
}

/// Verify session - for Caddy forward_auth
async fn session_verify(
    headers: HeaderMap,
    State(_state): State<AppState>,
) -> Result<Response> {
    use crate::services::decode_token;

    // Get cookie
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    // Parse kubarr_session cookie
    let token = cookie_header
        .split(';')
        .filter_map(|c| {
            let c = c.trim();
            if c.starts_with("kubarr_session=") {
                Some(c.strip_prefix("kubarr_session=").unwrap())
            } else {
                None
            }
        })
        .next();

    let token = match token {
        Some(t) => t,
        None => return Err(AppError::Unauthorized("No session".to_string())),
    };

    // Verify token
    let claims = decode_token(token)?;

    // Return user info in headers for Caddy to forward
    let mut response_headers = HeaderMap::new();
    response_headers.insert("X-Auth-User-Id", HeaderValue::from_str(&claims.sub).unwrap_or(HeaderValue::from_static("")));
    if let Some(email) = &claims.email {
        response_headers.insert("X-Auth-User-Email", HeaderValue::from_str(email).unwrap_or(HeaderValue::from_static("")));
    }

    Ok((response_headers, "OK").into_response())
}

/// Logout - clears session cookie
async fn session_logout() -> Response {
    let cookie = "kubarr_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0";

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(cookie).unwrap());

    (headers, Json(serde_json::json!({"success": true}))).into_response()
}

/// OAuth2 authorization endpoint
async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<AuthorizeQuery>,
) -> Result<Response> {
    use crate::services::security::decode_token;

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

    // Check for existing session cookie
    let session_user = extract_session_user(&headers, &state).await;

    if let Some(found_user) = session_user {
        // User is already logged in - create auth code and redirect to callback
        tracing::info!("User {} already logged in via session, creating auth code", found_user.email);

        let code = oauth2_service
            .create_authorization_code(
                &params.client_id,
                found_user.id,
                &params.redirect_uri,
                params.scope.as_deref(),
                params.code_challenge.as_deref(),
                params.code_challenge_method.as_deref(),
                600, // 10 minutes expiry
            )
            .await?;

        // Redirect to callback with authorization code
        let callback_url = if params.redirect_uri.contains('?') {
            format!("{}&code={}&state={}", params.redirect_uri, code, params.state.unwrap_or_default())
        } else {
            format!("{}?code={}&state={}", params.redirect_uri, code, params.state.unwrap_or_default())
        };

        return Ok(Redirect::to(&callback_url).into_response());
    }

    // No valid session - redirect to login page
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

/// Extract user from session cookie (kubarr_session)
async fn extract_session_user(headers: &HeaderMap, state: &AppState) -> Option<user::Model> {
    use crate::services::security::decode_token;

    // Get cookie header
    let cookie_header = headers.get(axum::http::header::COOKIE)?;
    let cookies = cookie_header.to_str().ok()?;

    // Find kubarr_session cookie
    let token = cookies
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie.strip_prefix("kubarr_session=").map(|v| v.to_string())
        })?;

    // Decode and validate the token
    let claims = decode_token(&token).ok()?;

    // Get user from database
    let user_id = claims.sub.parse::<i64>().ok()?;
    User::find_by_id(user_id)
        .filter(user::Column::IsActive.eq(true))
        .filter(user::Column::IsApproved.eq(true))
        .one(&state.db)
        .await
        .ok()
        .flatten()
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
    let oauth2_service = OAuth2Service::new(&state.db);

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
    let oauth2_service = OAuth2Service::new(&state.db);

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
            description: Set(Some("OAuth2-proxy client secret (for syncing to Kubernetes)".to_string())),
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
