use axum::{
    extract::{Query, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::api::extractors::{AdminUser, AuthUser};
use crate::config::CONFIG;
use crate::db::entities::audit_log::{AuditAction, ResourceType};
use crate::db::entities::prelude::*;
use crate::db::entities::{
    invite, oauth2_client, pending_2fa_challenge, role, system_setting, user, user_role,
};
use crate::error::{AppError, Result};
use crate::services::{
    generate_2fa_challenge_token, get_jwks, hash_password, verify_password, verify_totp,
    OAuth2Service,
};
use crate::state::AppState;

/// Create auth routes
pub fn auth_routes(state: AppState) -> Router {
    Router::new()
        // Login endpoints - redirect to frontend, handle form POST
        .route("/login", get(login_page).post(login_submit))
        // Registration endpoint
        .route("/register", get(register_page).post(register_submit))
        // 2FA verification page for OAuth flow
        .route("/2fa", get(twofa_page).post(twofa_submit))
        // OAuth2/OIDC endpoints
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/introspect", post(introspect))
        .route("/userinfo", get(userinfo))
        .route("/revoke", post(revoke))
        .route("/jwks", get(jwks))
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        // Direct API login (returns token in body - legacy)
        .route("/api/login", post(api_login))
        // Session-based login (sets HttpOnly cookie)
        .route("/session/login", post(session_login))
        .route("/session/verify", get(session_verify))
        .route("/session/logout", post(session_logout))
        .route("/session/2fa/verify", post(verify_2fa_challenge))
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

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum SessionLoginResponse {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "2fa_required")]
    TwoFactorRequired { challenge_token: String },
    #[serde(rename = "2fa_setup_required")]
    TwoFactorSetupRequired,
}

#[derive(Debug, Deserialize)]
pub struct Verify2FARequest {
    pub challenge_token: String,
    pub code: String,
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

/// Login page - serves HTML login form directly
async fn login_page(Query(params): Query<LoginPageQuery>) -> Response {
    let error_html = params
        .error
        .as_ref()
        .map(|err| {
            format!(
                r#"<div class="rounded-md bg-red-900 p-4 mb-4">
            <div class="text-sm text-red-200">{}</div>
        </div>"#,
                html_escape(err)
            )
        })
        .unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
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

    // Check if 2FA is enabled
    if found_user.totp_enabled {
        use chrono::Duration;

        // Create a pending 2FA challenge with OAuth params
        let challenge_token = generate_2fa_challenge_token();
        let expires_at = Utc::now() + Duration::minutes(5);

        // Store OAuth params in the challenge (we'll encode them in a simple way)
        // Delete any existing challenges for this user
        Pending2faChallenge::delete_many()
            .filter(pending_2fa_challenge::Column::UserId.eq(found_user.id))
            .exec(&state.db)
            .await?;

        let challenge = pending_2fa_challenge::ActiveModel {
            user_id: Set(found_user.id),
            challenge_token: Set(challenge_token.clone()),
            expires_at: Set(expires_at),
            ..Default::default()
        };
        challenge.insert(&state.db).await?;

        // Redirect to 2FA page with challenge token and OAuth params
        let twofa_url = format!(
            "/auth/2fa?challenge_token={}&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}",
            urlencoding::encode(&challenge_token),
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
        );
        return Ok(Redirect::to(&twofa_url).into_response());
    }

    // Check if 2FA is required by role but not enabled
    if user_requires_2fa(&state.db, found_user.id).await && !found_user.totp_enabled {
        let error_url = format!(
            "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Two-factor authentication required. Please set up 2FA in your account settings.")
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

// ============================================================================
// 2FA Page for OAuth Flow
// ============================================================================

#[derive(Debug, Deserialize)]
struct TwoFAPageQuery {
    challenge_token: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TwoFAFormData {
    challenge_token: String,
    code: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

/// 2FA verification page for OAuth flow
async fn twofa_page(Query(params): Query<TwoFAPageQuery>) -> Html<String> {
    let error_html = if let Some(error) = &params.error {
        format!(
            r#"<div class="rounded-md bg-red-100 dark:bg-red-900/50 border border-red-300 dark:border-red-700 p-4 mb-6">
                <div class="text-sm text-red-700 dark:text-red-200">{}</div>
            </div>"#,
            html_escape(&error)
        )
    } else {
        String::new()
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Two-Factor Authentication - Kubarr</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {{
            darkMode: 'class',
        }}
    </script>
</head>
<body class="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center px-4">
    <div class="max-w-md w-full space-y-8">
        <div class="text-center">
            <div class="flex justify-center mb-4">
                <div class="p-3 bg-blue-100 dark:bg-blue-900/30 rounded-full">
                    <svg class="w-8 h-8 text-blue-600 dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"></path>
                    </svg>
                </div>
            </div>
            <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                Two-Factor Authentication
            </h2>
            <p class="mt-2 text-sm text-gray-600 dark:text-gray-400">
                Enter the 6-digit code from your authenticator app
            </p>
        </div>

        {error_html}

        <form class="mt-8 space-y-6" method="POST" action="/auth/2fa">
            <input type="hidden" name="challenge_token" value="{challenge_token}">
            <input type="hidden" name="client_id" value="{client_id}">
            <input type="hidden" name="redirect_uri" value="{redirect_uri}">
            <input type="hidden" name="scope" value="{scope}">
            <input type="hidden" name="state" value="{state}">
            <input type="hidden" name="code_challenge" value="{code_challenge}">
            <input type="hidden" name="code_challenge_method" value="{code_challenge_method}">

            <div class="flex justify-center">
                <input
                    type="text"
                    name="code"
                    inputmode="numeric"
                    pattern="[0-9]*"
                    autocomplete="one-time-code"
                    placeholder="000000"
                    maxlength="6"
                    autofocus
                    required
                    class="w-48 px-4 py-3 text-center text-2xl font-mono tracking-[0.5em] border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                />
            </div>

            <button
                type="submit"
                id="submit-btn"
                class="group relative w-full flex justify-center py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 transition-colors disabled:opacity-50"
            >
                <span id="btn-text">Verify</span>
                <span id="btn-loading" class="hidden items-center">
                    <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    Verifying...
                </span>
            </button>
        </form>
    </div>
    <script>
        const input = document.querySelector('input[name="code"]');
        const form = document.querySelector('form');
        const submitBtn = document.getElementById('submit-btn');
        const btnText = document.getElementById('btn-text');
        const btnLoading = document.getElementById('btn-loading');

        // Only allow digits
        input.addEventListener('input', function(e) {{
            this.value = this.value.replace(/\D/g, '').slice(0, 6);

            // Auto-submit when 6 digits entered
            if (this.value.length === 6) {{
                // Show loading state
                btnText.classList.add('hidden');
                btnLoading.classList.remove('hidden');
                btnLoading.classList.add('flex');
                submitBtn.disabled = true;
                // Don't disable input - disabled inputs are not submitted!
                input.readOnly = true;

                // Small delay for visual feedback
                setTimeout(() => form.submit(), 100);
            }}
        }});

        // Show loading on manual submit too
        form.addEventListener('submit', function() {{
            btnText.classList.add('hidden');
            btnLoading.classList.remove('hidden');
            btnLoading.classList.add('flex');
            submitBtn.disabled = true;
            // Use readOnly instead of disabled to keep the value
            input.readOnly = true;
        }});
    </script>
</body>
</html>"#,
        error_html = error_html,
        challenge_token = html_escape(&params.challenge_token),
        client_id = html_escape(&params.client_id),
        redirect_uri = html_escape(&params.redirect_uri),
        scope = html_escape(&params.scope),
        state = html_escape(&params.state),
        code_challenge = html_escape(&params.code_challenge),
        code_challenge_method = html_escape(&params.code_challenge_method),
    );

    Html(html)
}

/// 2FA form submission for OAuth flow
async fn twofa_submit(
    State(state): State<AppState>,
    Form(form): Form<TwoFAFormData>,
) -> Result<Response> {
    // Find the challenge
    let challenge = Pending2faChallenge::find()
        .filter(pending_2fa_challenge::Column::ChallengeToken.eq(&form.challenge_token))
        .one(&state.db)
        .await?;

    let challenge = match challenge {
        Some(c) => c,
        None => {
            let error_url = format!(
                "/auth/2fa?challenge_token={}&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
                urlencoding::encode(&form.challenge_token),
                urlencoding::encode(&form.client_id),
                urlencoding::encode(&form.redirect_uri),
                urlencoding::encode(&form.scope),
                urlencoding::encode(&form.state),
                urlencoding::encode(&form.code_challenge),
                urlencoding::encode(&form.code_challenge_method),
                urlencoding::encode("Invalid or expired challenge. Please try logging in again.")
            );
            return Ok(Redirect::to(&error_url).into_response());
        }
    };

    // Check if challenge is expired
    if challenge.expires_at < Utc::now() {
        challenge.clone().delete(&state.db).await?;
        let error_url = format!(
            "/auth/login?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Challenge has expired. Please try logging in again.")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Get the user
    let found_user = User::find_by_id(challenge.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    // Get the TOTP secret
    let totp_secret = found_user
        .totp_secret
        .as_ref()
        .ok_or_else(|| AppError::Internal("User has 2FA enabled but no secret".to_string()))?;

    // Verify the TOTP code
    if !verify_totp(totp_secret, &form.code, &found_user.email)? {
        let error_url = format!(
            "/auth/2fa?challenge_token={}&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}&error={}",
            urlencoding::encode(&form.challenge_token),
            urlencoding::encode(&form.client_id),
            urlencoding::encode(&form.redirect_uri),
            urlencoding::encode(&form.scope),
            urlencoding::encode(&form.state),
            urlencoding::encode(&form.code_challenge),
            urlencoding::encode(&form.code_challenge_method),
            urlencoding::encode("Invalid verification code")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Delete the challenge (it's been used)
    challenge.delete(&state.db).await?;

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
    use crate::api::extractors::{get_user_app_access, get_user_permissions};
    use crate::services::create_access_token;

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

/// Check if user's role requires 2FA
async fn user_requires_2fa(db: &sea_orm::DatabaseConnection, user_id: i64) -> bool {
    let roles: Vec<role::Model> = Role::find()
        .inner_join(UserRole)
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    roles.iter().any(|r| r.requires_2fa)
}

/// Complete login by setting session cookie
async fn complete_session_login(state: &AppState, found_user: &user::Model) -> Result<Response> {
    use crate::api::extractors::{get_user_app_access, get_user_permissions};
    use crate::services::create_access_token;

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

    Ok((headers, Json(SessionLoginResponse::Success)).into_response())
}

/// Session-based login - sets HttpOnly cookie or returns 2FA challenge
async fn session_login(
    State(state): State<AppState>,
    Json(login): Json<LoginRequest>,
) -> Result<Response> {
    use chrono::Duration;

    // Find user
    let found_user = User::find()
        .filter(user::Column::Username.eq(&login.username))
        .one(&state.db)
        .await?;

    let found_user = match found_user {
        Some(u) => u,
        None => {
            // Log failed login attempt
            let _ = state.audit.log_failure(
                AuditAction::LoginFailed,
                ResourceType::Session,
                None,
                None,
                Some(login.username.clone()),
                Some(serde_json::json!({"reason": "user_not_found"})),
                None,
                None,
                "Invalid username or password",
            ).await;

            return Err(AppError::Unauthorized(
                "Invalid username or password".to_string(),
            ))
        }
    };

    // Verify password
    if !verify_password(&login.password, &found_user.hashed_password) {
        // Log failed login attempt
        let _ = state.audit.log_failure(
            AuditAction::LoginFailed,
            ResourceType::Session,
            None,
            Some(found_user.id),
            Some(found_user.username.clone()),
            Some(serde_json::json!({"reason": "invalid_password"})),
            None,
            None,
            "Invalid username or password",
        ).await;

        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Check if user is active
    if !found_user.is_active {
        let _ = state.audit.log_failure(
            AuditAction::LoginFailed,
            ResourceType::Session,
            None,
            Some(found_user.id),
            Some(found_user.username.clone()),
            Some(serde_json::json!({"reason": "account_inactive"})),
            None,
            None,
            "Account is inactive",
        ).await;

        return Err(AppError::Forbidden("Account is inactive".to_string()));
    }

    // Check if user is approved
    if !found_user.is_approved {
        let _ = state.audit.log_failure(
            AuditAction::LoginFailed,
            ResourceType::Session,
            None,
            Some(found_user.id),
            Some(found_user.username.clone()),
            Some(serde_json::json!({"reason": "account_not_approved"})),
            None,
            None,
            "Account pending approval",
        ).await;

        return Err(AppError::Forbidden("Account pending approval".to_string()));
    }

    // Check if user has 2FA enabled
    if found_user.totp_enabled {
        // Create a 2FA challenge
        let challenge_token = generate_2fa_challenge_token();
        let now = Utc::now();
        let expires_at = now + Duration::minutes(5);

        // Clean up any existing challenges for this user
        Pending2faChallenge::delete_many()
            .filter(pending_2fa_challenge::Column::UserId.eq(found_user.id))
            .exec(&state.db)
            .await?;

        // Create new challenge
        let challenge = pending_2fa_challenge::ActiveModel {
            user_id: Set(found_user.id),
            challenge_token: Set(challenge_token.clone()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            ..Default::default()
        };
        challenge.insert(&state.db).await?;

        return Ok(
            Json(SessionLoginResponse::TwoFactorRequired { challenge_token }).into_response(),
        );
    }

    // Check if role requires 2FA but user hasn't set it up
    if user_requires_2fa(&state.db, found_user.id).await {
        return Ok(Json(SessionLoginResponse::TwoFactorSetupRequired).into_response());
    }

    // Log successful login
    let _ = state.audit.log_success(
        AuditAction::Login,
        ResourceType::Session,
        Some(found_user.id.to_string()),
        Some(found_user.id),
        Some(found_user.username.clone()),
        None,
        None,
        None,
    ).await;

    // No 2FA required - complete login
    complete_session_login(&state, &found_user).await
}

/// Verify 2FA code and complete login
async fn verify_2fa_challenge(
    State(state): State<AppState>,
    Json(request): Json<Verify2FARequest>,
) -> Result<Response> {
    // Find the challenge
    let challenge = Pending2faChallenge::find()
        .filter(pending_2fa_challenge::Column::ChallengeToken.eq(&request.challenge_token))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid or expired challenge".to_string()))?;

    // Check if challenge is expired
    if challenge.expires_at < Utc::now() {
        // Delete expired challenge
        challenge.clone().delete(&state.db).await?;
        return Err(AppError::BadRequest("Challenge has expired".to_string()));
    }

    // Get the user
    let found_user = User::find_by_id(challenge.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    // Get the TOTP secret
    let totp_secret = found_user
        .totp_secret
        .as_ref()
        .ok_or_else(|| AppError::Internal("User has 2FA enabled but no secret".to_string()))?;

    // Verify the TOTP code
    if !verify_totp(totp_secret, &request.code, &found_user.email)? {
        // Log failed 2FA attempt
        let _ = state.audit.log_failure(
            AuditAction::TwoFactorFailed,
            ResourceType::Session,
            None,
            Some(found_user.id),
            Some(found_user.username.clone()),
            Some(serde_json::json!({"reason": "invalid_code"})),
            None,
            None,
            "Invalid verification code",
        ).await;

        return Err(AppError::BadRequest(
            "Invalid verification code".to_string(),
        ));
    }

    // Delete the challenge (it's been used)
    challenge.delete(&state.db).await?;

    // Log successful 2FA verification and login
    let _ = state.audit.log_success(
        AuditAction::TwoFactorVerified,
        ResourceType::Session,
        Some(found_user.id.to_string()),
        Some(found_user.id),
        Some(found_user.username.clone()),
        None,
        None,
        None,
    ).await;

    let _ = state.audit.log_success(
        AuditAction::Login,
        ResourceType::Session,
        Some(found_user.id.to_string()),
        Some(found_user.id),
        Some(found_user.username.clone()),
        Some(serde_json::json!({"method": "2fa"})),
        None,
        None,
    ).await;

    // Complete the login
    complete_session_login(&state, &found_user).await
}

/// Verify session - for Caddy forward_auth
async fn session_verify(headers: HeaderMap, State(_state): State<AppState>) -> Result<Response> {
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
    response_headers.insert(
        "X-Auth-User-Id",
        HeaderValue::from_str(&claims.sub).unwrap_or(HeaderValue::from_static("")),
    );
    if let Some(email) = &claims.email {
        response_headers.insert(
            "X-Auth-User-Email",
            HeaderValue::from_str(email).unwrap_or(HeaderValue::from_static("")),
        );
    }

    Ok((response_headers, "OK").into_response())
}

/// Logout - clears session cookie
async fn session_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Try to extract user from session for audit logging
    if let Some(found_user) = extract_session_user(&headers, &state).await {
        let _ = state.audit.log_success(
            AuditAction::Logout,
            ResourceType::Session,
            Some(found_user.id.to_string()),
            Some(found_user.id),
            Some(found_user.username.clone()),
            None,
            None,
            None,
        ).await;
    }

    let cookie = "kubarr_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0";

    let mut response_headers = HeaderMap::new();
    response_headers.insert(SET_COOKIE, HeaderValue::from_str(cookie).unwrap());

    (response_headers, Json(serde_json::json!({"success": true}))).into_response()
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
        tracing::info!(
            "User {} already logged in via session, creating auth code",
            found_user.email
        );

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
            format!(
                "{}&code={}&state={}",
                params.redirect_uri,
                code,
                params.state.unwrap_or_default()
            )
        } else {
            format!(
                "{}?code={}&state={}",
                params.redirect_uri,
                code,
                params.state.unwrap_or_default()
            )
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
    let token = cookies.split(';').find_map(|cookie| {
        let cookie = cookie.trim();
        cookie
            .strip_prefix("kubarr_session=")
            .map(|v| v.to_string())
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

// ============================================================================
// Registration
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RegisterPageQuery {
    pub invite: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterFormData {
    pub username: String,
    pub email: String,
    pub password: String,
    pub invite_code: String,
}

/// Registration page - serves HTML registration form
async fn register_page(Query(params): Query<RegisterPageQuery>) -> Response {
    let error_html = params
        .error
        .as_ref()
        .map(|err| {
            format!(
                r#"<div class="rounded-md bg-red-900 p-4 mb-4">
            <div class="text-sm text-red-200">{}</div>
        </div>"#,
                html_escape(err)
            )
        })
        .unwrap_or_default();

    let invite_code = params.invite.as_deref().unwrap_or("");

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Kubarr - Register</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="min-h-screen bg-gray-900 flex items-center justify-center px-4">
    <div class="max-w-md w-full space-y-8">
        <div>
            <h2 class="mt-6 text-center text-3xl font-extrabold text-white">
                Kubarr Dashboard
            </h2>
            <p class="mt-2 text-center text-sm text-gray-400">
                Create your account
            </p>
        </div>
        <form class="mt-8 space-y-6" method="POST" action="/auth/register">
            {}
            <div class="rounded-md shadow-sm space-y-3">
                <div>
                    <label for="username" class="block text-sm font-medium text-gray-300 mb-1">Username</label>
                    <input id="username" name="username" type="text" required autofocus
                        class="appearance-none relative block w-full px-3 py-2 border border-gray-700 placeholder-gray-500 text-white bg-gray-800 rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                        placeholder="Choose a username" />
                </div>
                <div>
                    <label for="email" class="block text-sm font-medium text-gray-300 mb-1">Email</label>
                    <input id="email" name="email" type="email" required
                        class="appearance-none relative block w-full px-3 py-2 border border-gray-700 placeholder-gray-500 text-white bg-gray-800 rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                        placeholder="you@example.com" />
                </div>
                <div>
                    <label for="password" class="block text-sm font-medium text-gray-300 mb-1">Password</label>
                    <input id="password" name="password" type="password" required minlength="8"
                        class="appearance-none relative block w-full px-3 py-2 border border-gray-700 placeholder-gray-500 text-white bg-gray-800 rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                        placeholder="Minimum 8 characters" />
                </div>
                <input type="hidden" name="invite_code" value="{}">
            </div>
            <div>
                <button type="submit"
                    class="group relative w-full flex justify-center py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500">
                    Create Account
                </button>
            </div>
            <div class="text-center">
                <a href="/login" class="text-sm text-blue-400 hover:text-blue-300">
                    Already have an account? Sign in
                </a>
            </div>
        </form>
    </div>
</body>
</html>"#,
        error_html,
        html_escape(invite_code),
    );

    Html(html).into_response()
}

/// Registration form submission
async fn register_submit(
    State(state): State<AppState>,
    Form(form): Form<RegisterFormData>,
) -> Result<Response> {
    // Validate invite code
    let found_invite = Invite::find()
        .filter(invite::Column::Code.eq(&form.invite_code))
        .one(&state.db)
        .await?;

    let found_invite = match found_invite {
        Some(inv) => inv,
        None => {
            let error_url = format!(
                "/auth/register?invite={}&error={}",
                urlencoding::encode(&form.invite_code),
                urlencoding::encode("Invalid invite code")
            );
            return Ok(Redirect::to(&error_url).into_response());
        }
    };

    // Check if invite is already used
    if found_invite.is_used {
        let error_url = format!(
            "/auth/register?invite={}&error={}",
            urlencoding::encode(&form.invite_code),
            urlencoding::encode("This invite code has already been used")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Check if invite is expired
    if let Some(expires_at) = found_invite.expires_at {
        if expires_at < Utc::now() {
            let error_url = format!(
                "/auth/register?invite={}&error={}",
                urlencoding::encode(&form.invite_code),
                urlencoding::encode("This invite code has expired")
            );
            return Ok(Redirect::to(&error_url).into_response());
        }
    }

    // Validate password length
    if form.password.len() < 8 {
        let error_url = format!(
            "/auth/register?invite={}&error={}",
            urlencoding::encode(&form.invite_code),
            urlencoding::encode("Password must be at least 8 characters")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Check if username is taken
    let existing_user = User::find()
        .filter(user::Column::Username.eq(&form.username))
        .one(&state.db)
        .await?;

    if existing_user.is_some() {
        let error_url = format!(
            "/auth/register?invite={}&error={}",
            urlencoding::encode(&form.invite_code),
            urlencoding::encode("Username is already taken")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Check if email is taken
    let existing_email = User::find()
        .filter(user::Column::Email.eq(&form.email))
        .one(&state.db)
        .await?;

    if existing_email.is_some() {
        let error_url = format!(
            "/auth/register?invite={}&error={}",
            urlencoding::encode(&form.invite_code),
            urlencoding::encode("Email is already registered")
        );
        return Ok(Redirect::to(&error_url).into_response());
    }

    // Hash password
    let password_hash = hash_password(&form.password)?;

    // Create user (auto-approved since they have a valid invite)
    let now = Utc::now();
    let new_user = user::ActiveModel {
        username: Set(form.username.clone()),
        email: Set(form.email.clone()),
        hashed_password: Set(password_hash),
        is_active: Set(true),
        is_approved: Set(true), // Auto-approve invited users
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let created_user = new_user.insert(&state.db).await?;

    // Mark invite as used
    let mut invite_model: invite::ActiveModel = found_invite.into();
    invite_model.is_used = Set(true);
    invite_model.used_by_id = Set(Some(created_user.id));
    invite_model.used_at = Set(Some(now));
    invite_model.update(&state.db).await?;

    // Log the registration
    let _ = state
        .audit
        .log_success(
            AuditAction::UserCreated,
            ResourceType::User,
            Some(created_user.id.to_string()),
            Some(created_user.id),
            Some(form.username.clone()),
            Some(serde_json::json!({"method": "invite"})),
            None,
            None,
        )
        .await;

    // Redirect to login with success message
    Ok(Redirect::to("/login").into_response())
}
