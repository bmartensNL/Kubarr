use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Kubernetes error: {0}")]
    Kubernetes(#[from] kube::Error),

    #[error("Kubernetes config error: {0}")]
    KubeConfig(#[from] kube::config::KubeconfigError),

    #[error("Kubernetes in-cluster config error: {0}")]
    KubeInCluster(#[from] kube::config::InClusterError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),

    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
}

#[derive(Serialize)]
struct ErrorResponse {
    detail: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::Kubernetes(e) => {
                tracing::error!("Kubernetes error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Kubernetes error: {}", e),
                )
            }
            AppError::KubeConfig(e) => {
                tracing::error!("Kubernetes config error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Kubernetes config error: {}", e),
                )
            }
            AppError::KubeInCluster(e) => {
                tracing::error!("Kubernetes in-cluster config error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Kubernetes in-cluster config error: {}", e),
                )
            }
            AppError::Json(e) => (StatusCode::BAD_REQUEST, format!("JSON error: {}", e)),
            AppError::Yaml(e) => (StatusCode::BAD_REQUEST, format!("YAML error: {}", e)),
            AppError::Io(e) => {
                tracing::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {}", e))
            }
            AppError::Jwt(e) => (StatusCode::UNAUTHORIZED, format!("JWT error: {}", e)),
            AppError::Bcrypt(e) => {
                tracing::error!("Bcrypt error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Authentication error".to_string(),
                )
            }
            AppError::HttpClient(e) => {
                tracing::error!("HTTP client error: {}", e);
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Upstream service error: {}", e),
                )
            }
        };

        (status, Json(ErrorResponse { detail: message })).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    async fn get_response_body(response: Response) -> (StatusCode, String) {
        let status = response.status();
        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(bytes.to_vec()).unwrap();
        (status, body_str)
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let error = AppError::NotFound("User not found".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("User not found"));
    }

    #[tokio::test]
    async fn test_bad_request_error() {
        let error = AppError::BadRequest("Invalid input".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("Invalid input"));
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let error = AppError::Unauthorized("Token expired".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("Token expired"));
    }

    #[tokio::test]
    async fn test_forbidden_error() {
        let error = AppError::Forbidden("Admin access required".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("Admin access required"));
    }

    #[tokio::test]
    async fn test_conflict_error() {
        let error = AppError::Conflict("Username already exists".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body.contains("Username already exists"));
    }

    #[tokio::test]
    async fn test_internal_error() {
        let error = AppError::Internal("Something went wrong".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body.contains("Something went wrong"));
    }

    #[tokio::test]
    async fn test_service_unavailable_error() {
        let error = AppError::ServiceUnavailable("Service down".to_string());
        let response = error.into_response();
        let (status, body) = get_response_body(response).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("Service down"));
    }

    #[tokio::test]
    async fn test_json_error_response_format() {
        let error = AppError::NotFound("Resource not found".to_string());
        let response = error.into_response();
        let (_, body) = get_response_body(response).await;

        // Response should be JSON with "detail" field
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(parsed.get("detail").is_some());
        assert_eq!(parsed.get("detail").unwrap(), "Resource not found");
    }

    #[test]
    fn test_error_display_impl() {
        assert_eq!(
            AppError::NotFound("test".to_string()).to_string(),
            "Not found: test"
        );
        assert_eq!(
            AppError::BadRequest("test".to_string()).to_string(),
            "Bad request: test"
        );
        assert_eq!(
            AppError::Unauthorized("test".to_string()).to_string(),
            "Unauthorized: test"
        );
        assert_eq!(
            AppError::Forbidden("test".to_string()).to_string(),
            "Forbidden: test"
        );
        assert_eq!(
            AppError::Conflict("test".to_string()).to_string(),
            "Conflict: test"
        );
        assert_eq!(
            AppError::Internal("test".to_string()).to_string(),
            "Internal server error: test"
        );
        assert_eq!(
            AppError::ServiceUnavailable("test".to_string()).to_string(),
            "Service unavailable: test"
        );
    }

    #[test]
    fn test_json_error_from_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_err.is_err());
        let app_error: AppError = json_err.unwrap_err().into();
        assert!(matches!(app_error, AppError::Json(_)));
    }

    #[test]
    fn test_io_error_from_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_error: AppError = io_err.into();
        assert!(matches!(app_error, AppError::Io(_)));
    }
}
