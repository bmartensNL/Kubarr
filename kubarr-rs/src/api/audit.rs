use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use crate::api::extractors::{user_has_permission, AuthUser};
use crate::error::{AppError, Result};
use crate::services::audit::{get_audit_logs, get_audit_stats, clear_old_logs, AuditLogQuery, AuditLogResponse, AuditStats};
use crate::state::AppState;

/// Create audit routes
pub fn audit_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(list_audit_logs))
        .route("/stats", get(audit_stats))
        .route("/clear", axum::routing::post(clear_audit_logs))
        .with_state(state)
}

/// List audit logs with filtering and pagination
async fn list_audit_logs(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogResponse>> {
    if !user_has_permission(&state.db, user.id, "audit.view").await {
        return Err(AppError::Forbidden("Permission denied: audit.view required".to_string()));
    }
    let logs = get_audit_logs(&state.db, query).await?;
    Ok(Json(logs))
}

/// Get audit statistics
async fn audit_stats(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<AuditStats>> {
    if !user_has_permission(&state.db, user.id, "audit.view").await {
        return Err(AppError::Forbidden("Permission denied: audit.view required".to_string()));
    }
    let stats = get_audit_stats(&state.db).await?;
    Ok(Json(stats))
}

/// Clear old audit logs (admin only)
#[derive(serde::Deserialize)]
pub struct ClearLogsRequest {
    pub days: Option<i64>,
}

#[derive(serde::Serialize)]
pub struct ClearLogsResponse {
    pub deleted: u64,
    pub message: String,
}

async fn clear_audit_logs(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(request): Json<ClearLogsRequest>,
) -> Result<Json<ClearLogsResponse>> {
    if !user_has_permission(&state.db, user.id, "audit.manage").await {
        return Err(AppError::Forbidden("Permission denied: audit.manage required".to_string()));
    }
    let days = request.days.unwrap_or(90); // Default to 90 days retention
    let deleted = clear_old_logs(&state.db, days).await?;

    Ok(Json(ClearLogsResponse {
        deleted,
        message: format!("Deleted {} audit log entries older than {} days", deleted, days),
    }))
}
