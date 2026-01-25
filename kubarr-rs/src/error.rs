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
    Database(#[from] sqlx::Error),

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
