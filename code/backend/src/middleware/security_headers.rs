//! HTTP Security Headers Middleware
//!
//! Injects standard security response headers on every response.

use axum::{
    body::Body,
    http::{HeaderValue, Request, Response},
    middleware::Next,
};

// Compile-time constant header values — all are valid ASCII so `from_static` is safe.
const X_FRAME_OPTIONS: HeaderValue = HeaderValue::from_static("DENY");
const X_CONTENT_TYPE_OPTIONS: HeaderValue = HeaderValue::from_static("nosniff");
const REFERRER_POLICY: HeaderValue =
    HeaderValue::from_static("strict-origin-when-cross-origin");
const X_XSS_PROTECTION: HeaderValue = HeaderValue::from_static("1; mode=block");
const CONTENT_SECURITY_POLICY: HeaderValue = HeaderValue::from_static(
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'",
);

/// Axum middleware function that injects HTTP security headers into every response.
pub async fn add_security_headers(req: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert("x-frame-options", X_FRAME_OPTIONS);
    headers.insert("x-content-type-options", X_CONTENT_TYPE_OPTIONS);
    headers.insert("referrer-policy", REFERRER_POLICY);
    headers.insert("x-xss-protection", X_XSS_PROTECTION);
    headers.insert("content-security-policy", CONTENT_SECURITY_POLICY);

    response
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use tower::ServiceExt;

    async fn noop_handler() -> &'static str {
        "ok"
    }

    fn test_app() -> Router {
        Router::new()
            .route("/test", get(noop_handler))
            .layer(middleware::from_fn(add_security_headers))
    }

    async fn get_response_headers(path: &str) -> axum::http::HeaderMap {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response.headers().clone()
    }

    #[tokio::test]
    async fn test_x_frame_options_header() {
        let headers = get_response_headers("/test").await;
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn test_x_content_type_options_header() {
        let headers = get_response_headers("/test").await;
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    }

    #[tokio::test]
    async fn test_referrer_policy_header() {
        let headers = get_response_headers("/test").await;
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }

    #[tokio::test]
    async fn test_x_xss_protection_header() {
        let headers = get_response_headers("/test").await;
        assert_eq!(headers.get("x-xss-protection").unwrap(), "1; mode=block");
    }

    #[tokio::test]
    async fn test_content_security_policy_header() {
        let headers = get_response_headers("/test").await;
        assert_eq!(
            headers.get("content-security-policy").unwrap(),
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"
        );
    }

    #[tokio::test]
    async fn test_all_five_headers_present() {
        let headers = get_response_headers("/test").await;
        assert!(headers.contains_key("x-frame-options"));
        assert!(headers.contains_key("x-content-type-options"));
        assert!(headers.contains_key("referrer-policy"));
        assert!(headers.contains_key("x-xss-protection"));
        assert!(headers.contains_key("content-security-policy"));
    }
}
