use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use crate::error::Result;
use crate::middleware::permissions::{AuditManage, AuditView, Authorized};
use crate::services::audit::{
    clear_old_logs, get_audit_logs, get_audit_stats, AuditLogQuery, AuditLogResponse, AuditStats,
};
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
    _auth: Authorized<AuditView>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogResponse>> {
    let logs = get_audit_logs(&state.db, query).await?;
    Ok(Json(logs))
}

/// Get audit statistics
async fn audit_stats(
    State(state): State<AppState>,
    _auth: Authorized<AuditView>,
) -> Result<Json<AuditStats>> {
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
    _auth: Authorized<AuditManage>,
    Json(request): Json<ClearLogsRequest>,
) -> Result<Json<ClearLogsResponse>> {
    let days = request.days.unwrap_or(90); // Default to 90 days retention
    let deleted = clear_old_logs(&state.db, days).await?;

    Ok(Json(ClearLogsResponse {
        deleted,
        message: format!(
            "Deleted {} audit log entries older than {} days",
            deleted, days
        ),
    }))
}
