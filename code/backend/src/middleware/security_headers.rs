//! HTTP security headers middleware
//!
//! Adds standard security headers to every HTTP response to protect against
//! common web vulnerabilities such as clickjacking, MIME sniffing, and XSS.

use axum::{extract::Request, middleware::Next, response::Response};
use http::HeaderValue;

/// Middleware that injects HTTP security headers into every response.
///
/// Headers applied:
/// - `X-Frame-Options: DENY` — prevents clickjacking
/// - `X-Content-Type-Options: nosniff` — prevents MIME-type sniffing
/// - `Referrer-Policy: strict-origin-when-cross-origin` — limits referrer info
/// - `X-XSS-Protection: 1; mode=block` — legacy XSS filter hint
/// - `Content-Security-Policy` — restricts resource loading to same origin
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        "x-frame-options",
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "x-xss-protection",
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'",
        ),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, middleware, routing::get, Router};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn dummy_handler() -> &'static str {
        "ok"
    }

    fn test_app() -> Router {
        Router::new()
            .route("/test", get(dummy_handler))
            .layer(middleware::from_fn(security_headers))
    }

    #[tokio::test]
    async fn test_x_frame_options_deny() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("x-frame-options").unwrap(),
            "DENY"
        );
    }

    #[tokio::test]
    async fn test_x_content_type_options_nosniff() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn test_referrer_policy() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }

    #[tokio::test]
    async fn test_xss_protection() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-xss-protection").unwrap(),
            "1; mode=block"
        );
    }

    #[tokio::test]
    async fn test_content_security_policy() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let csp = response
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("script-src 'self'"));
    }

    #[tokio::test]
    async fn test_all_five_headers_present() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let headers = response.headers();
        assert!(
            headers.contains_key("x-frame-options"),
            "missing x-frame-options"
        );
        assert!(
            headers.contains_key("x-content-type-options"),
            "missing x-content-type-options"
        );
        assert!(
            headers.contains_key("referrer-policy"),
            "missing referrer-policy"
        );
        assert!(
            headers.contains_key("x-xss-protection"),
            "missing x-xss-protection"
        );
        assert!(
            headers.contains_key("content-security-policy"),
            "missing content-security-policy"
        );
    }
}
