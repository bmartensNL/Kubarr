//! App proxy endpoints
//!
//! Reverse proxy requests to installed apps under /proxy/{app_name}/*

use axum::{
    extract::{Path, Request, State, WebSocketUpgrade},
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
    ws_upgrade: Option<WebSocketUpgrade>,
    request: Request,
) -> Result<Response> {
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body = request.into_body();
    proxy_app_inner(
        state,
        app_name,
        String::new(),
        auth,
        method,
        headers,
        ws_upgrade,
        body,
    )
    .await
}

/// Proxy requests to an installed app (with path)
async fn proxy_app_with_path(
    State(state): State<AppState>,
    Path((app_name, path)): Path<(String, String)>,
    auth: Authenticated,
    ws_upgrade: Option<WebSocketUpgrade>,
    request: Request,
) -> Result<Response> {
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body = request.into_body();
    proxy_app_inner(
        state, app_name, path, auth, method, headers, ws_upgrade, body,
    )
    .await
}

/// Inner proxy implementation
#[allow(clippy::too_many_arguments)]
async fn proxy_app_inner(
    state: AppState,
    app_name: String,
    path: String,
    auth: Authenticated,
    method: Method,
    headers: HeaderMap,
    ws_upgrade: Option<WebSocketUpgrade>,
    body: Body,
) -> Result<Response> {
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
