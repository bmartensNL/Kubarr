//! Frontend proxy handler
//!
//! Proxies unmatched requests to the frontend service.
//! Also handles app proxying for authenticated users at /{app_name}/* paths.
//! Implements SPA routing: returns index.html for non-asset 404s.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, Method, Response, StatusCode},
};

use crate::config::CONFIG;
use crate::error::{AppError, Result};
use crate::services::security::decode_token;
use crate::state::AppState;

/// Check if a path looks like a static asset (has a file extension)
fn is_static_asset(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    if let Some(last_segment) = path.rsplit('/').next() {
        last_segment.contains('.')
    } else {
        false
    }
}

/// Reserved paths that should never be treated as app names
const RESERVED_PATHS: &[&str] = &["api", "auth", "proxy", "assets", "favicon.svg", "login", "setup"];

/// Extract app name from path if it looks like an app path
fn extract_app_name(path: &str) -> Option<&str> {
    let path = path.trim_start_matches('/');
    let first_segment = path.split('/').next()?;

    // Skip reserved paths and static assets
    if first_segment.is_empty()
        || RESERVED_PATHS.contains(&first_segment)
        || first_segment.contains('.') {
        return None;
    }

    Some(first_segment)
}

/// Extract session token from cookie header
fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?;
    let cookie_str = cookies.to_str().ok()?;

    for cookie in cookie_str.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix("kubarr_session=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Proxy requests to the frontend service with SPA routing support
/// Also handles app proxying for authenticated users at /{app_name}/* paths
pub async fn proxy_frontend(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response<Body>> {
    let path = request.uri().path().to_string();
    let query = request
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    let method = request.method().clone();
    let headers = request.headers().clone();

    // Check if this looks like an app path and user is authenticated
    if let Some(app_name) = extract_app_name(&path) {
        if let Some(token) = extract_session_token(&headers) {
            if let Ok(claims) = decode_token(&token) {
                if let Ok(user_id) = claims.sub.parse::<i64>() {
                    // Check if user has access to this app
                    if check_app_permission(&state, user_id, app_name).await {
                        // Try to proxy to the app
                        if let Ok(target_url) = get_app_target_url(&state, app_name, &path, &query).await {
                            tracing::debug!("Proxying to app {}: {}", app_name, target_url);
                            let body = request.into_body();
                            let proxy = &state.proxy;
                            return proxy.proxy_http(&target_url, method, headers, body).await;
                        }
                    }
                }
            }
        }
    }

    let body = request.into_body();
    let proxy = &state.proxy;

    // For static assets, proxy directly
    if is_static_asset(&path) {
        let target_url = format!("{}{}{}", CONFIG.frontend_url, path, query);
        tracing::debug!("Proxying static asset: {}", target_url);

        return proxy
            .proxy_http(&target_url, method, headers, body)
            .await
            .map_err(|e| {
                tracing::error!("Frontend proxy error: {}", e);
                AppError::BadGateway(format!("Frontend unavailable: {}", e))
            });
    }

    // For non-asset paths, try the path first, fall back to index.html on 404
    let target_url = format!("{}{}{}", CONFIG.frontend_url, path, query);
    tracing::debug!("Proxying to frontend: {}", target_url);

    let response = proxy
        .proxy_http(&target_url, method.clone(), headers.clone(), body)
        .await
        .map_err(|e| {
            tracing::error!("Frontend proxy error: {}", e);
            AppError::BadGateway(format!("Frontend unavailable: {}", e))
        })?;

    // If 404, serve index.html for SPA routing
    if response.status() == StatusCode::NOT_FOUND {
        let index_url = format!("{}/index.html", CONFIG.frontend_url);
        tracing::debug!("SPA fallback to index.html for path: {}", path);

        return proxy
            .proxy_http(&index_url, Method::GET, headers, Body::empty())
            .await
            .map_err(|e| {
                tracing::error!("Frontend proxy error (index.html): {}", e);
                AppError::BadGateway(format!("Frontend unavailable: {}", e))
            });
    }

    Ok(response)
}

/// Check if user has permission to access the app
async fn check_app_permission(state: &AppState, user_id: i64, app_name: &str) -> bool {
    use crate::endpoints::extractors::get_user_permissions;

    let permissions = get_user_permissions(&state.db, user_id).await;

    // Check for app.* wildcard or specific app.{name} permission
    permissions.contains(&"app.*".to_string())
        || permissions.contains(&format!("app.{}", app_name))
}

/// Get the target URL for an app
async fn get_app_target_url(state: &AppState, app_name: &str, path: &str, query: &str) -> Result<String> {
    // Check cache first
    let base_url = if let Some(cached_url) = state.endpoint_cache.get(app_name).await {
        cached_url
    } else {
        // Get K8s client
        let k8s_guard = state.k8s_client.read().await;
        let k8s = k8s_guard
            .as_ref()
            .ok_or_else(|| AppError::ServiceUnavailable("Kubernetes not available".to_string()))?;

        // Get service endpoints for the app
        // Apps are deployed in namespaces named after the app
        let endpoints = k8s.get_service_endpoints(app_name, app_name).await?;

        if endpoints.is_empty() {
            return Err(AppError::NotFound(format!(
                "App {} not found or not ready",
                app_name
            )));
        }

        // Use the first endpoint
        let endpoint = &endpoints[0];

        // Build the internal URL
        let base_url = format!(
            "http://{}.{}.svc.cluster.local:{}",
            endpoint.name, endpoint.namespace, endpoint.port
        );

        // Cache the base URL
        state.endpoint_cache.set(app_name, base_url.clone()).await;

        base_url
    };

    // Strip the app name prefix from the path
    let app_path = path
        .trim_start_matches('/')
        .strip_prefix(app_name)
        .unwrap_or("")
        .trim_start_matches('/');

    if app_path.is_empty() && query.is_empty() {
        Ok(format!("{}/", base_url))
    } else if app_path.is_empty() {
        Ok(format!("{}/?{}", base_url, query.trim_start_matches('?')))
    } else {
        Ok(format!("{}/{}{}", base_url, app_path, query))
    }
}
