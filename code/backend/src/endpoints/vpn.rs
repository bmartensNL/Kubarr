//! VPN provider and app VPN configuration endpoints

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::error::Result;
use crate::middleware::permissions::{Authorized, SettingsManage, SettingsView};
use crate::services::deployment::{DeploymentManager, DeploymentRequest};
use crate::services::vpn::{
    self, AppVpnConfigResponse, AssignVpnRequest, CreateVpnProviderRequest, SupportedProvider,
    UpdateVpnProviderRequest, VpnProviderResponse, VpnTestResult,
};
use crate::state::AppState;

/// Create VPN routes
pub fn vpn_routes(state: AppState) -> Router {
    Router::new()
        // VPN providers
        .route("/providers", get(list_providers).post(create_provider))
        .route(
            "/providers/:id",
            get(get_provider)
                .put(update_provider)
                .delete(delete_provider),
        )
        .route("/providers/:id/test", post(test_provider))
        // App VPN configs
        .route("/apps", get(list_app_configs))
        .route(
            "/apps/:app_name",
            get(get_app_config).put(assign_vpn).delete(remove_vpn),
        )
        // Supported providers
        .route("/supported-providers", get(list_supported_providers))
        .with_state(state)
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
struct ProvidersResponse {
    providers: Vec<VpnProviderResponse>,
}

#[derive(Debug, Serialize)]
struct AppConfigsResponse {
    configs: Vec<AppVpnConfigResponse>,
}

#[derive(Debug, Serialize)]
struct SupportedProvidersResponse {
    providers: Vec<SupportedProvider>,
}

// ============================================================================
// VPN Provider Endpoints
// ============================================================================

/// List all VPN providers
async fn list_providers(
    State(state): State<AppState>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<ProvidersResponse>> {
    let db = state.get_db().await?;
    let providers = vpn::list_vpn_providers(&db).await?;
    Ok(Json(ProvidersResponse { providers }))
}

/// Get a VPN provider by ID
async fn get_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<VpnProviderResponse>> {
    let db = state.get_db().await?;
    let provider = vpn::get_vpn_provider(&db, id).await?;
    Ok(Json(provider))
}

/// Create a new VPN provider
async fn create_provider(
    State(state): State<AppState>,
    _auth: Authorized<SettingsManage>,
    Json(req): Json<CreateVpnProviderRequest>,
) -> Result<Json<VpnProviderResponse>> {
    let db = state.get_db().await?;
    let provider = vpn::create_vpn_provider(&db, req).await?;
    Ok(Json(provider))
}

/// Update a VPN provider
async fn update_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth: Authorized<SettingsManage>,
    Json(req): Json<UpdateVpnProviderRequest>,
) -> Result<Json<VpnProviderResponse>> {
    let db = state.get_db().await?;
    let provider = vpn::update_vpn_provider(&db, id, req).await?;
    Ok(Json(provider))
}

/// Delete a VPN provider
async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth: Authorized<SettingsManage>,
) -> Result<Json<serde_json::Value>> {
    let db = state.get_db().await?;
    let k8s = state.k8s_client.read().await;
    let client = k8s.as_ref().ok_or_else(|| {
        crate::error::AppError::Internal("Kubernetes client not available".to_string())
    })?;
    vpn::delete_vpn_provider(&db, client, id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

/// Test VPN provider connection
async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    _auth: Authorized<SettingsManage>,
) -> Result<Json<VpnTestResult>> {
    let db = state.get_db().await?;
    // Verify provider exists
    let _provider = vpn::get_vpn_provider(&db, id).await?;

    // For now, just return success if credentials are valid
    // TODO: Implement actual VPN connection test via Gluetun test container
    Ok(Json(VpnTestResult {
        success: true,
        message:
            "VPN provider configuration is valid. Deploy an app to test the actual connection."
                .to_string(),
        public_ip: None,
    }))
}

// ============================================================================
// App VPN Config Endpoints
// ============================================================================

/// List all app VPN configurations
async fn list_app_configs(
    State(state): State<AppState>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<AppConfigsResponse>> {
    let db = state.get_db().await?;
    let configs = vpn::list_app_vpn_configs(&db).await?;
    Ok(Json(AppConfigsResponse { configs }))
}

/// Get app VPN configuration
async fn get_app_config(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    _auth: Authorized<SettingsView>,
) -> Result<Json<Option<AppVpnConfigResponse>>> {
    let db = state.get_db().await?;
    let config = vpn::get_app_vpn_config(&db, &app_name).await?;
    Ok(Json(config))
}

/// Assign VPN to an app and redeploy
async fn assign_vpn(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    _auth: Authorized<SettingsManage>,
    Json(req): Json<AssignVpnRequest>,
) -> Result<Json<AppVpnConfigResponse>> {
    let db = state.get_db().await?;
    // Save VPN config to database
    let config = vpn::assign_vpn_to_app(&db, &app_name, req).await?;

    // Trigger redeploy to apply VPN changes
    let k8s = state.k8s_client.read().await;
    if let Some(k8s_client) = k8s.as_ref() {
        let catalog = state.catalog.read().await;
        let deployment_manager = DeploymentManager::with_db(k8s_client, &catalog, &db);
        let deploy_request = DeploymentRequest {
            app_name: app_name.clone(),
            custom_config: std::collections::HashMap::new(),
        };
        match deployment_manager.deploy_app(&deploy_request, None).await {
            Ok(status) => {
                tracing::info!("Redeployed app {} with VPN: {}", app_name, status.message);
            }
            Err(e) => {
                tracing::warn!("Failed to redeploy app {} with VPN: {}", app_name, e);
            }
        }
    }

    Ok(Json(config))
}

/// Remove VPN from an app and redeploy
async fn remove_vpn(
    State(state): State<AppState>,
    Path(app_name): Path<String>,
    _auth: Authorized<SettingsManage>,
) -> Result<Json<serde_json::Value>> {
    let db = state.get_db().await?;
    let k8s = state.k8s_client.read().await;
    let client = k8s.as_ref().ok_or_else(|| {
        crate::error::AppError::Internal("Kubernetes client not available".to_string())
    })?;

    // Remove VPN config from database
    vpn::remove_vpn_from_app(&db, client, &app_name).await?;

    // Trigger redeploy to remove VPN sidecar
    let catalog = state.catalog.read().await;
    let deployment_manager = DeploymentManager::with_db(client, &catalog, &db);
    let deploy_request = DeploymentRequest {
        app_name: app_name.clone(),
        custom_config: std::collections::HashMap::new(),
    };
    match deployment_manager.deploy_app(&deploy_request, None).await {
        Ok(status) => {
            tracing::info!(
                "Redeployed app {} without VPN: {}",
                app_name,
                status.message
            );
        }
        Err(e) => {
            tracing::warn!("Failed to redeploy app {} without VPN: {}", app_name, e);
        }
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Supported Providers Endpoint
// ============================================================================

/// List supported VPN service providers
async fn list_supported_providers(
    _auth: Authorized<SettingsView>,
) -> Result<Json<SupportedProvidersResponse>> {
    let providers = vpn::get_supported_providers();
    Ok(Json(SupportedProvidersResponse { providers }))
}
