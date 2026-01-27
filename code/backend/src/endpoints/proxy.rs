//! App proxy endpoints
//!
//! Reverse proxy requests to installed apps under /proxy/{app_name}/*

use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{HeaderMap, Method},
    response::Response,
    routing::any,
    Router,
};

use crate::error::{AppError, Result};
use crate::middleware::Authenticated;
use crate::services::{is_websocket_upgrade, proxy_websocket, ProxyService};
use crate::state::AppState;

/// Create proxy routes
pub fn proxy_routes(state: AppState) -> Router {
    Router::new()
        .route("/{app_name}", any(proxy_app))
        .route("/{app_name}/", any(proxy_app))
        .route("/{app_name}/*path", any(proxy_app))
        .with_state(state)
}

/// Proxy requests to an installed app
async fn proxy_app(
    State(state): State<AppState>,
    Path((app_name, path)): Path<(String, Option<String>)>,
    auth: Authenticated,
    method: Method,
    headers: HeaderMap,
    ws_upgrade: Option<WebSocketUpgrade>,
    body: Body,
) -> Result<Response> {
    let path = path.unwrap_or_default();
    let user = auth.user();

    // Get AuthenticatedUser from request extensions to check permissions
    // Note: Authenticated extractor already validates auth, but we need full permissions
    // Since we're using the Authenticated extractor, the user is already authenticated
    // We need to check app access separately

    // Check if user has access to this app
    // Get the auth user with permissions from the middleware
    if !check_app_permission(&state, user.id, &app_name).await {
        return Err(AppError::Forbidden(format!(
            "No access to app: {}",
            app_name
        )));
    }

    // Get the app's service endpoint from Kubernetes
    let target_url = get_app_target_url(&state, &app_name, &path).await?;

    // Handle WebSocket upgrade
    if let Some(ws) = ws_upgrade {
        if is_websocket_upgrade(&headers) {
            return Ok(proxy_websocket(ws, target_url).await);
        }
    }

    // Proxy HTTP request
    let proxy = ProxyService::new();
    proxy.proxy_http(&target_url, method, headers, body).await
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
async fn get_app_target_url(state: &AppState, app_name: &str, path: &str) -> Result<String> {
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
    // Format: http://{service_name}.{namespace}.svc.cluster.local:{port}/{path}
    let base_url = format!(
        "http://{}.{}.svc.cluster.local:{}",
        endpoint.name, endpoint.namespace, endpoint.port
    );

    // Append path
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        Ok(base_url)
    } else {
        Ok(format!("{}/{}", base_url, path))
    }
}
