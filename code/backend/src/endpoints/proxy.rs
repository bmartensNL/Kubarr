//! App proxy endpoints
//!
//! Reverse proxy requests to installed apps under /proxy/{app_name}/*

use axum::{
    body::Body,
    extract::ws::WebSocketUpgrade,
    extract::{FromRequestParts, Path, Request, State},
    http::{HeaderMap, Method},
    response::Response,
    routing::any,
    Router,
};

use crate::error::{AppError, Result};
use crate::middleware::Authenticated;
use crate::services::{is_websocket_upgrade, proxy_websocket};
use crate::state::AppState;

/// Create proxy routes
pub fn proxy_routes(state: AppState) -> Router {
    Router::new()
        .route("/{app_name}", any(proxy_app_root))
        .route("/{app_name}/", any(proxy_app_root))
        .route("/{app_name}/{*path}", any(proxy_app_with_path))
        .with_state(state)
}

/// Proxy requests to an installed app (root path)
async fn proxy_app_root(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    auth: Authenticated,
    request: Request,
) -> Result<Response> {
    proxy_app_request(state, app_name, String::new(), auth, request).await
}

/// Proxy requests to an installed app (with path)
async fn proxy_app_with_path(
    State(state): State<AppState>,
    Path((app_name, path)): Path<(String, String)>,
    auth: Authenticated,
    request: Request,
) -> Result<Response> {
    proxy_app_request(state, app_name, path, auth, request).await
}

/// Inner proxy implementation
async fn proxy_app_request(
    state: AppState,
    app_name: String,
    path: String,
    auth: Authenticated,
    request: Request,
) -> Result<Response> {
    let user = auth.user();

    // Check if user has access to this app
    if !check_app_permission(&state, user.id, &app_name).await {
        return Err(AppError::Forbidden(format!(
            "No access to app: {}",
            app_name
        )));
    }

    // Get the app's service endpoint from Kubernetes
    let target_url = get_app_target_url(&state, &app_name, &path).await?;

    let method = request.method().clone();
    let headers = request.headers().clone();

    // Handle WebSocket upgrade
    if is_websocket_upgrade(&headers) {
        let (mut parts, body) = request.into_parts();
        if let Ok(ws) = WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
            return Ok(proxy_websocket(ws, target_url).await);
        }
        // If WebSocket extraction failed, reconstruct and fall through to HTTP proxy
        let request = Request::from_parts(parts, body);
        let body = request.into_body();
        return state
            .proxy
            .proxy_http(&target_url, method, headers, body)
            .await;
    }

    // Proxy HTTP request
    let body = request.into_body();
    state
        .proxy
        .proxy_http(&target_url, method, headers, body)
        .await
}

/// Check if user has permission to access the app
async fn check_app_permission(state: &AppState, user_id: i64, app_name: &str) -> bool {
    use crate::endpoints::extractors::get_user_permissions;

    let db = match state.get_db().await {
        Ok(db) => db,
        Err(_) => return false,
    };
    let permissions = get_user_permissions(&db, user_id).await;

    // Check for app.* wildcard or specific app.{name} permission
    permissions.contains(&"app.*".to_string()) || permissions.contains(&format!("app.{}", app_name))
}

/// Get the target URL for an app
async fn get_app_target_url(state: &AppState, app_name: &str, path: &str) -> Result<String> {
    // Check cache first
    let (base_url, _base_path) = if let Some(cached) = state.endpoint_cache.get(app_name).await {
        cached
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
        // Format: http://{service_name}.{namespace}.svc.cluster.local:{port}/{path}
        let base_url = format!(
            "http://{}.{}.svc.cluster.local:{}",
            endpoint.name, endpoint.namespace, endpoint.port
        );

        let base_path = endpoint.base_path.clone();

        // Cache the endpoint
        state
            .endpoint_cache
            .set(app_name, base_url.clone(), base_path.clone())
            .await;

        (base_url, base_path)
    };

    // Append path
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        Ok(base_url)
    } else {
        Ok(format!("{}/{}", base_url, path))
    }
}
