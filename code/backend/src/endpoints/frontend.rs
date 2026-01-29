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
use crate::models::prelude::*;
use crate::models::session;
use crate::services::security::decode_session_token;
use crate::state::AppState;
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

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
const RESERVED_PATHS: &[&str] = &[
    "api",
    "auth",
    "proxy",
    "assets",
    "favicon.svg",
    "login",
    "setup",
    "app-error",
];

/// Extract app name from path if it looks like an app path
fn extract_app_name(path: &str) -> Option<&str> {
    let path = path.trim_start_matches('/');
    let first_segment = path.split('/').next()?;

    // Skip reserved paths and static assets
    if first_segment.is_empty()
        || RESERVED_PATHS.contains(&first_segment)
        || first_segment.contains('.')
    {
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

/// Rewrite Location headers in app proxy responses so redirects go through the proxy.
/// For apps without base_path: rewrite internal URLs and prepend app name to absolute paths.
/// For apps with base_path: only rewrite internal URLs (absolute paths already have correct prefix).
fn rewrite_app_response(
    mut response: Response<Body>,
    app_name: &str,
    internal_base_url: &str,
    has_base_path: bool,
) -> Response<Body> {
    if !response.status().is_redirection() {
        return response;
    }

    let location = match response.headers().get(header::LOCATION) {
        Some(loc) => match loc.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return response,
        },
        None => return response,
    };

    let rewritten = if let Some(path) = location.strip_prefix(internal_base_url) {
        // Absolute internal URL: http://app.ns.svc.cluster.local:PORT/path → /app_name/path
        let path = path.trim_start_matches('/');
        if has_base_path {
            // For base_path apps, the path already contains the base path prefix
            // (e.g., /jackett/UI/Login) — don't add app_name again
            format!("/{}", path)
        } else {
            format!("/{}/{}", app_name, path)
        }
    } else if !has_base_path && location.starts_with('/') {
        // Absolute path without base_path: prepend the app prefix
        // (apps with base_path already have the correct prefix in their redirects)
        format!("/{}{}", app_name, location)
    } else {
        // Relative or full external URL, or base_path app with correct absolute path
        location
    };

    if let Ok(val) = rewritten.parse() {
        response.headers_mut().insert(header::LOCATION, val);
    }

    response
}

/// Rewrite text response bodies for apps without base_path.
/// Handles HTML, JavaScript, and CSS responses to prefix absolute paths with /{app_name}
/// so the browser fetches resources through the proxy instead of hitting the SPA fallback.
async fn rewrite_response_body(response: Response<Body>, app_name: &str) -> Response<Body> {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let is_html = content_type.contains("text/html");
    let is_js = content_type.contains("javascript");
    let is_css = content_type.contains("text/css");

    if !is_html && !is_js && !is_css {
        return response;
    }

    let (parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    let text = String::from_utf8_lossy(&bytes);
    let prefix = format!("/{}", app_name);

    let rewritten = if is_html {
        rewrite_html(&text, &prefix)
    } else if is_js {
        rewrite_js(&text, &prefix)
    } else {
        rewrite_css(&text, &prefix)
    };

    let body_bytes = rewritten.into_bytes();
    let mut parts = parts;
    if parts.headers.contains_key(header::CONTENT_LENGTH) {
        if let Ok(val) = body_bytes.len().to_string().parse() {
            parts.headers.insert(header::CONTENT_LENGTH, val);
        }
    }

    Response::from_parts(parts, Body::from(body_bytes))
}

/// Rewrite HTML content: absolute paths in attributes + inline script patterns
fn rewrite_html(html: &str, prefix: &str) -> String {
    // Rewrite absolute paths in HTML attributes (double and single quoted)
    let rewritten = html
        .replace("src=\"/", &format!("src=\"{}/", prefix))
        .replace("href=\"/", &format!("href=\"{}/", prefix))
        .replace("action=\"/", &format!("action=\"{}/", prefix))
        .replace("src='/", &format!("src='{}/", prefix))
        .replace("href='/", &format!("href='{}/", prefix))
        .replace("action='/", &format!("action='{}/", prefix));

    // Fix protocol-relative URLs that got incorrectly rewritten (e.g., //cdn.example.com)
    let rewritten = rewritten
        .replace(&format!("src=\"{}//", prefix), "src=\"//")
        .replace(&format!("href=\"{}//", prefix), "href=\"//")
        .replace(&format!("action=\"{}//", prefix), "action=\"//")
        .replace(&format!("src='{}//", prefix), "src='//")
        .replace(&format!("href='{}//", prefix), "href='//")
        .replace(&format!("action='{}//", prefix), "action='//");

    // Rewrite inline JS patterns commonly found in <script> blocks:
    // - JSON config values like "base": "/" or "basePath": "/"
    //   These are used by apps (e.g., Deluge ExtJS) to construct URLs at runtime
    rewrite_js_paths(&rewritten, prefix)
}

/// Rewrite JavaScript content: webpack public paths and other absolute path patterns
fn rewrite_js(js: &str, prefix: &str) -> String {
    rewrite_js_paths(js, prefix)
}

/// Rewrite CSS content: url() references with absolute paths
fn rewrite_css(css: &str, prefix: &str) -> String {
    // url("/path/...") → url("/app_name/path/...")
    let rewritten = css
        .replace("url(\"/", &format!("url(\"{}/", prefix))
        .replace("url('/", &format!("url('{}/", prefix))
        .replace("url(/", &format!("url({}/", prefix));

    // Fix protocol-relative
    rewritten
        .replace(&format!("url(\"{}//'", prefix), "url(\"//")
        .replace(&format!("url('{}//'", prefix), "url('//")
        .replace(&format!("url({}//'", prefix), "url(//")
}

/// Rewrite common JS patterns containing absolute paths.
/// This is generic and handles patterns from various frameworks:
/// - Webpack minified public path: .p="/..." (e.g., .p="/_next/")
/// - JSON-style config values: :"/" or : "/" (e.g., "base": "/")
fn rewrite_js_paths(text: &str, prefix: &str) -> String {
    let rewritten = text
        // Webpack public path (minified): .p="/..." or .p='/'
        .replace(".p=\"/", &format!(".p=\"{}/", prefix))
        .replace(".p='/", &format!(".p='{}/", prefix))
        // JSON config values: :"/" and : "/" (with optional space after colon)
        // Catches patterns like "base":"/" or "base": "/"
        .replace(":\"/", &format!(":\"{}/", prefix))
        .replace(":'/", &format!(":\'{}/", prefix))
        .replace(": \"/", &format!(": \"{}/", prefix))
        .replace(": '/", &format!(": '{}/", prefix));

    // Fix protocol-relative URLs that got incorrectly rewritten
    let rewritten = rewritten
        .replace(&format!(".p=\"{}//", prefix), ".p=\"//")
        .replace(&format!(".p='{}//", prefix), ".p='//")
        .replace(&format!(":\"{}//'", prefix), ":\"//")
        .replace(&format!(":\'{}//'", prefix), ":'//")
        .replace(&format!(": \"{}//'", prefix), ": \"//")
        .replace(&format!(": '{}//'", prefix), ": '//");

    // Fix double-prefixing: if a path already had the prefix, undo the extra one
    let double_prefix = format!("{}{}/", prefix, prefix);
    let single_prefix = format!("{}/", prefix);
    rewritten.replace(&double_prefix, &single_prefix)
}

/// Create a redirect response to the app error page
fn redirect_to_app_error(app_name: &str, reason: &str, details: &str) -> Response<Body> {
    let encoded_details = urlencoding::encode(details);
    let redirect_url = format!(
        "/app-error?app={}&reason={}&details={}",
        app_name, reason, encoded_details
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, redirect_url)
        .body(Body::empty())
        .unwrap()
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
        tracing::info!("Detected potential app path: {}", app_name);
        if let Some(token) = extract_session_token(&headers) {
            tracing::info!("Found session token for app {}", app_name);
            // Decode session token (contains session ID, not user ID)
            if let Ok(claims) = decode_session_token(&token) {
                tracing::info!(
                    "Session token decoded for app {}, sid={}",
                    app_name,
                    claims.sid
                );
                // Look up session in database to get user_id
                let db = match state.get_db().await {
                    Ok(db) => db,
                    Err(_) => {
                        tracing::warn!("Database not available for session lookup");
                        return proxy_to_frontend(
                            &state,
                            path,
                            query,
                            method,
                            headers,
                            request.into_body(),
                        )
                        .await;
                    }
                };
                if let Ok(Some(session_record)) = Session::find()
                    .filter(session::Column::Id.eq(&claims.sid))
                    .filter(session::Column::IsRevoked.eq(false))
                    .filter(session::Column::ExpiresAt.gt(Utc::now()))
                    .one(&db)
                    .await
                {
                    let user_id = session_record.user_id;
                    tracing::info!(
                        "Session valid for user {} accessing app {}",
                        user_id,
                        app_name
                    );
                    // Check if user has access to this app
                    let has_permission = check_app_permission(&state, user_id, app_name).await;
                    tracing::info!(
                        "User {} permission for app {}: {}",
                        user_id,
                        app_name,
                        has_permission
                    );
                    if has_permission {
                        // Try to proxy to the app
                        match get_app_target_url(&state, app_name, &path, &query).await {
                            Ok((base_url, has_base_path, target_url)) => {
                                tracing::info!(
                                    "Proxying to app {}: {} (base_path: {})",
                                    app_name,
                                    target_url,
                                    has_base_path
                                );
                                let body = request.into_body();
                                let proxy = &state.proxy;
                                match proxy.proxy_http(&target_url, method, headers, body).await {
                                    Ok(response) => {
                                        // Rewrite Location headers for redirects
                                        let response = rewrite_app_response(
                                            response,
                                            app_name,
                                            &base_url,
                                            has_base_path,
                                        );
                                        // For apps without base_path, rewrite HTML body
                                        // to prefix absolute paths with /{app_name}
                                        let response = if !has_base_path {
                                            rewrite_response_body(response, app_name).await
                                        } else {
                                            response
                                        };
                                        return Ok(response);
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to connect to app {}: {}",
                                            app_name,
                                            e
                                        );
                                        return Ok(redirect_to_app_error(
                                            app_name,
                                            "connection_failed",
                                            &e.to_string(),
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::info!(
                                    "App {} not found, falling through to frontend: {}",
                                    app_name,
                                    e
                                );
                                // Don't show error page - fall through to frontend proxy
                                // The path might be a frontend route (e.g., /networking)
                            }
                        }
                    }
                } else {
                    tracing::info!("Session not found or invalid for app {}", app_name);
                }
            } else {
                tracing::info!("Failed to decode session token for app {}", app_name);
            }
        } else {
            tracing::info!("No session token for app path {}", app_name);
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

    let db = match state.get_db().await {
        Ok(db) => db,
        Err(_) => return false,
    };
    let permissions = get_user_permissions(&db, user_id).await;

    // Check for app.* wildcard or specific app.{name} permission
    permissions.contains(&"app.*".to_string()) || permissions.contains(&format!("app.{}", app_name))
}

/// Helper function to proxy to frontend
async fn proxy_to_frontend(
    state: &AppState,
    path: String,
    query: String,
    method: Method,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Result<Response<Body>> {
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

/// Get the target URL for an app, returning (base_url, has_base_path, full_target_url)
async fn get_app_target_url(
    state: &AppState,
    app_name: &str,
    path: &str,
    query: &str,
) -> Result<(String, bool, String)> {
    // Check cache first
    let (base_url, base_path) = if let Some(cached) = state.endpoint_cache.get(app_name).await {
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

    let has_base_path = base_path.is_some();

    let target_url = if has_base_path {
        // App has a URL base (e.g., Sonarr at /sonarr) - keep the full path
        let trimmed = path.trim_start_matches('/');
        if trimmed.is_empty() && query.is_empty() {
            format!("{}/", base_url)
        } else if trimmed.is_empty() {
            format!("{}/?{}", base_url, query.trim_start_matches('?'))
        } else {
            format!("{}/{}{}", base_url, trimmed, query)
        }
    } else {
        // App has no URL base (e.g., qBittorrent) - strip the app name prefix
        let app_path = path
            .trim_start_matches('/')
            .strip_prefix(app_name)
            .unwrap_or("")
            .trim_start_matches('/');

        if app_path.is_empty() && query.is_empty() {
            format!("{}/", base_url)
        } else if app_path.is_empty() {
            format!("{}/?{}", base_url, query.trim_start_matches('?'))
        } else {
            format!("{}/{}{}", base_url, app_path, query)
        }
    };

    Ok((base_url, has_base_path, target_url))
}
