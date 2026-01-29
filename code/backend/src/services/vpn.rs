//! VPN Provider Service
//!
//! Manages VPN providers and app VPN configurations, including K8s secret generation
//! for Gluetun sidecar containers.

use std::collections::BTreeMap;

use chrono::Utc;
use k8s_openapi::api::core::v1::{
    Capabilities, Container, EnvVar, Pod, PodSpec, Secret, SecurityContext,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, PostParams};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::prelude::*;
use crate::models::{app_vpn_config, vpn_provider};
use crate::services::K8sClient;
use crate::state::DbConn;

// ============================================================================
// Credential Structures
// ============================================================================

/// WireGuard credentials for Gluetun
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardCredentials {
    pub private_key: String,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preshared_key: Option<String>,
}

/// OpenVPN credentials for Gluetun
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenVpnCredentials {
    pub username: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_countries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_cities: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_hostnames: Option<String>,
}

/// Unified credentials enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VpnCredentials {
    WireGuard(WireGuardCredentials),
    OpenVpn(OpenVpnCredentials),
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a VPN provider
#[derive(Debug, Clone, Deserialize)]
pub struct CreateVpnProviderRequest {
    pub name: String,
    pub vpn_type: vpn_provider::VpnType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_provider: Option<String>,
    pub credentials: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub kill_switch: bool,
    #[serde(default = "default_firewall_subnets")]
    pub firewall_outbound_subnets: String,
}

/// Request to update a VPN provider
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateVpnProviderRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kill_switch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firewall_outbound_subnets: Option<String>,
}

/// Response for a VPN provider (without credentials)
#[derive(Debug, Clone, Serialize)]
pub struct VpnProviderResponse {
    pub id: i64,
    pub name: String,
    pub vpn_type: vpn_provider::VpnType,
    pub service_provider: Option<String>,
    pub enabled: bool,
    pub kill_switch: bool,
    pub firewall_outbound_subnets: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    /// Number of apps using this provider
    pub app_count: usize,
}

/// Request to assign VPN to an app
#[derive(Debug, Clone, Deserialize)]
pub struct AssignVpnRequest {
    pub vpn_provider_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kill_switch_override: Option<bool>,
}

/// Response for app VPN configuration
#[derive(Debug, Clone, Serialize)]
pub struct AppVpnConfigResponse {
    pub app_name: String,
    pub vpn_provider_id: i64,
    pub vpn_provider_name: String,
    pub kill_switch_override: Option<bool>,
    pub effective_kill_switch: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

/// Supported VPN service provider info
#[derive(Debug, Clone, Serialize)]
pub struct SupportedProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub vpn_types: Vec<&'static str>,
    pub description: &'static str,
}

fn default_true() -> bool {
    true
}

fn default_firewall_subnets() -> String {
    "10.0.0.0/8,172.16.0.0/12,192.168.0.0/16".to_string()
}

// ============================================================================
// VPN Provider Functions
// ============================================================================

/// List all VPN providers
pub async fn list_vpn_providers(db: &DbConn) -> Result<Vec<VpnProviderResponse>> {
    let providers = VpnProvider::find().all(db).await?;

    // Get app counts for each provider
    let app_configs = AppVpnConfig::find().all(db).await?;

    let mut responses = Vec::new();
    for provider in providers {
        let app_count = app_configs
            .iter()
            .filter(|c| c.vpn_provider_id == provider.id)
            .count();

        responses.push(VpnProviderResponse {
            id: provider.id,
            name: provider.name,
            vpn_type: provider.vpn_type,
            service_provider: provider.service_provider,
            enabled: provider.enabled,
            kill_switch: provider.kill_switch,
            firewall_outbound_subnets: provider.firewall_outbound_subnets,
            created_at: provider.created_at,
            updated_at: provider.updated_at,
            app_count,
        });
    }

    Ok(responses)
}

/// Get a VPN provider by ID
pub async fn get_vpn_provider(db: &DbConn, id: i64) -> Result<VpnProviderResponse> {
    let provider = VpnProvider::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("VPN provider {} not found", id)))?;

    let app_count = AppVpnConfig::find()
        .filter(app_vpn_config::Column::VpnProviderId.eq(id))
        .count(db)
        .await? as usize;

    Ok(VpnProviderResponse {
        id: provider.id,
        name: provider.name,
        vpn_type: provider.vpn_type,
        service_provider: provider.service_provider,
        enabled: provider.enabled,
        kill_switch: provider.kill_switch,
        firewall_outbound_subnets: provider.firewall_outbound_subnets,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        app_count,
    })
}

/// Create a new VPN provider
pub async fn create_vpn_provider(
    db: &DbConn,
    req: CreateVpnProviderRequest,
) -> Result<VpnProviderResponse> {
    // Validate credentials based on VPN type
    validate_credentials(&req.vpn_type, &req.credentials)?;

    let now = Utc::now();
    let credentials_json = serde_json::to_string(&req.credentials)
        .map_err(|e| AppError::BadRequest(format!("Invalid credentials JSON: {}", e)))?;

    let new_provider = vpn_provider::ActiveModel {
        name: Set(req.name),
        vpn_type: Set(req.vpn_type),
        service_provider: Set(req.service_provider),
        credentials_json: Set(credentials_json),
        enabled: Set(req.enabled),
        kill_switch: Set(req.kill_switch),
        firewall_outbound_subnets: Set(req.firewall_outbound_subnets),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let provider = new_provider.insert(db).await?;

    Ok(VpnProviderResponse {
        id: provider.id,
        name: provider.name,
        vpn_type: provider.vpn_type,
        service_provider: provider.service_provider,
        enabled: provider.enabled,
        kill_switch: provider.kill_switch,
        firewall_outbound_subnets: provider.firewall_outbound_subnets,
        created_at: provider.created_at,
        updated_at: provider.updated_at,
        app_count: 0,
    })
}

/// Update a VPN provider
pub async fn update_vpn_provider(
    db: &DbConn,
    id: i64,
    req: UpdateVpnProviderRequest,
) -> Result<VpnProviderResponse> {
    let provider = VpnProvider::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("VPN provider {} not found", id)))?;

    let mut active_model: vpn_provider::ActiveModel = provider.clone().into();

    if let Some(name) = req.name {
        active_model.name = Set(name);
    }
    if let Some(service_provider) = req.service_provider {
        active_model.service_provider = Set(Some(service_provider));
    }
    if let Some(credentials) = req.credentials {
        validate_credentials(&provider.vpn_type, &credentials)?;
        let credentials_json = serde_json::to_string(&credentials)
            .map_err(|e| AppError::BadRequest(format!("Invalid credentials JSON: {}", e)))?;
        active_model.credentials_json = Set(credentials_json);
    }
    if let Some(enabled) = req.enabled {
        active_model.enabled = Set(enabled);
    }
    if let Some(kill_switch) = req.kill_switch {
        active_model.kill_switch = Set(kill_switch);
    }
    if let Some(firewall_outbound_subnets) = req.firewall_outbound_subnets {
        active_model.firewall_outbound_subnets = Set(firewall_outbound_subnets);
    }

    active_model.updated_at = Set(Utc::now());

    let updated = active_model.update(db).await?;

    let app_count = AppVpnConfig::find()
        .filter(app_vpn_config::Column::VpnProviderId.eq(id))
        .count(db)
        .await? as usize;

    Ok(VpnProviderResponse {
        id: updated.id,
        name: updated.name,
        vpn_type: updated.vpn_type,
        service_provider: updated.service_provider,
        enabled: updated.enabled,
        kill_switch: updated.kill_switch,
        firewall_outbound_subnets: updated.firewall_outbound_subnets,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        app_count,
    })
}

/// Delete a VPN provider
pub async fn delete_vpn_provider(db: &DbConn, k8s: &K8sClient, id: i64) -> Result<()> {
    // Get all apps using this provider
    let app_configs = AppVpnConfig::find()
        .filter(app_vpn_config::Column::VpnProviderId.eq(id))
        .all(db)
        .await?;

    // Delete K8s secrets for all associated apps
    for config in &app_configs {
        if let Err(e) = delete_vpn_secret_for_app(k8s, &config.app_name).await {
            tracing::warn!(
                "Failed to delete VPN secret for app {}: {}",
                config.app_name,
                e
            );
        }
    }

    // Delete the provider (cascade will delete app_vpn_configs)
    VpnProvider::delete_by_id(id).exec(db).await?;

    Ok(())
}

// ============================================================================
// App VPN Config Functions
// ============================================================================

/// List all app VPN configurations
pub async fn list_app_vpn_configs(db: &DbConn) -> Result<Vec<AppVpnConfigResponse>> {
    let configs = AppVpnConfig::find().all(db).await?;
    let providers = VpnProvider::find().all(db).await?;

    let provider_map: std::collections::HashMap<i64, &vpn_provider::Model> =
        providers.iter().map(|p| (p.id, p)).collect();

    let mut responses = Vec::new();
    for config in configs {
        if let Some(provider) = provider_map.get(&config.vpn_provider_id) {
            let effective_kill_switch = config.kill_switch_override.unwrap_or(provider.kill_switch);
            responses.push(AppVpnConfigResponse {
                app_name: config.app_name,
                vpn_provider_id: config.vpn_provider_id,
                vpn_provider_name: provider.name.clone(),
                kill_switch_override: config.kill_switch_override,
                effective_kill_switch,
                created_at: config.created_at,
                updated_at: config.updated_at,
            });
        }
    }

    Ok(responses)
}

/// Get app VPN configuration
pub async fn get_app_vpn_config(
    db: &DbConn,
    app_name: &str,
) -> Result<Option<AppVpnConfigResponse>> {
    let config = AppVpnConfig::find_by_id(app_name).one(db).await?;

    if let Some(config) = config {
        let provider = VpnProvider::find_by_id(config.vpn_provider_id)
            .one(db)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "VPN provider {} not found for app {}",
                    config.vpn_provider_id, app_name
                ))
            })?;

        let effective_kill_switch = config.kill_switch_override.unwrap_or(provider.kill_switch);
        Ok(Some(AppVpnConfigResponse {
            app_name: config.app_name,
            vpn_provider_id: config.vpn_provider_id,
            vpn_provider_name: provider.name,
            kill_switch_override: config.kill_switch_override,
            effective_kill_switch,
            created_at: config.created_at,
            updated_at: config.updated_at,
        }))
    } else {
        Ok(None)
    }
}

/// Assign VPN to an app
pub async fn assign_vpn_to_app(
    db: &DbConn,
    app_name: &str,
    req: AssignVpnRequest,
) -> Result<AppVpnConfigResponse> {
    // Verify provider exists and is enabled
    let provider = VpnProvider::find_by_id(req.vpn_provider_id)
        .one(db)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("VPN provider {} not found", req.vpn_provider_id))
        })?;

    if !provider.enabled {
        return Err(AppError::BadRequest(
            "Cannot assign a disabled VPN provider".to_string(),
        ));
    }

    let now = Utc::now();

    // Check if config already exists
    let existing = AppVpnConfig::find_by_id(app_name).one(db).await?;

    let config = if let Some(_existing) = existing {
        // Update existing
        let mut active_model = app_vpn_config::ActiveModel {
            app_name: Set(app_name.to_string()),
            vpn_provider_id: Set(req.vpn_provider_id),
            kill_switch_override: Set(req.kill_switch_override),
            updated_at: Set(now),
            ..Default::default()
        };
        active_model.created_at = sea_orm::ActiveValue::NotSet;
        active_model.update(db).await?
    } else {
        // Create new
        let new_config = app_vpn_config::ActiveModel {
            app_name: Set(app_name.to_string()),
            vpn_provider_id: Set(req.vpn_provider_id),
            kill_switch_override: Set(req.kill_switch_override),
            created_at: Set(now),
            updated_at: Set(now),
        };
        new_config.insert(db).await?
    };

    let effective_kill_switch = config.kill_switch_override.unwrap_or(provider.kill_switch);
    Ok(AppVpnConfigResponse {
        app_name: config.app_name,
        vpn_provider_id: config.vpn_provider_id,
        vpn_provider_name: provider.name,
        kill_switch_override: config.kill_switch_override,
        effective_kill_switch,
        created_at: config.created_at,
        updated_at: config.updated_at,
    })
}

/// Remove VPN from an app
pub async fn remove_vpn_from_app(db: &DbConn, k8s: &K8sClient, app_name: &str) -> Result<()> {
    // Delete K8s secret
    if let Err(e) = delete_vpn_secret_for_app(k8s, app_name).await {
        tracing::warn!("Failed to delete VPN secret for app {}: {}", app_name, e);
    }

    // Delete config
    AppVpnConfig::delete_by_id(app_name).exec(db).await?;

    Ok(())
}

// ============================================================================
// K8s Secret Management
// ============================================================================

/// Create or update a K8s secret for an app's VPN configuration
pub async fn create_vpn_secret_for_app(
    k8s: &K8sClient,
    db: &DbConn,
    app_name: &str,
) -> Result<String> {
    // Get app VPN config
    let config = AppVpnConfig::find_by_id(app_name)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No VPN config for app {}", app_name)))?;

    // Get provider
    let provider = VpnProvider::find_by_id(config.vpn_provider_id)
        .one(db)
        .await?
        .ok_or_else(|| {
            AppError::Internal(format!("VPN provider {} not found", config.vpn_provider_id))
        })?;

    // Parse credentials
    let credentials: serde_json::Value = serde_json::from_str(&provider.credentials_json)
        .map_err(|e| AppError::Internal(format!("Invalid credentials JSON: {}", e)))?;

    // Build secret data based on VPN type
    let mut secret_data: BTreeMap<String, String> = BTreeMap::new();

    // Set VPN type
    secret_data.insert("VPN_TYPE".to_string(), provider.vpn_type.to_string());

    // Set service provider if specified
    if let Some(ref service_provider) = provider.service_provider {
        if service_provider != "custom" {
            secret_data.insert("VPN_SERVICE_PROVIDER".to_string(), service_provider.clone());
        }
    }

    match provider.vpn_type {
        vpn_provider::VpnType::WireGuard => {
            build_wireguard_secret_data(&credentials, &mut secret_data)?;
        }
        vpn_provider::VpnType::OpenVpn => {
            build_openvpn_secret_data(&credentials, &mut secret_data)?;
        }
    }

    // Convert to base64-encoded data
    let encoded_data: BTreeMap<String, k8s_openapi::ByteString> = secret_data
        .into_iter()
        .map(|(k, v)| (k, k8s_openapi::ByteString(v.into_bytes())))
        .collect();

    let secret_name = format!("vpn-{}", app_name);
    let namespace = app_name;

    // Create secret object
    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(secret_name.clone()),
            namespace: Some(namespace.to_string()),
            labels: Some(BTreeMap::from([
                (
                    "app.kubernetes.io/managed-by".to_string(),
                    "kubarr".to_string(),
                ),
                (
                    "kubarr.io/vpn-provider".to_string(),
                    provider.id.to_string(),
                ),
            ])),
            ..Default::default()
        },
        data: Some(encoded_data),
        ..Default::default()
    };

    // Create or update in K8s
    let secrets: Api<Secret> = Api::namespaced(k8s.client().clone(), namespace);

    // Try to delete existing secret first (simpler than patch)
    let _ = secrets.delete(&secret_name, &DeleteParams::default()).await;

    // Create new secret
    secrets.create(&PostParams::default(), &secret).await?;

    Ok(secret_name)
}

/// Delete the VPN secret for an app
pub async fn delete_vpn_secret_for_app(k8s: &K8sClient, app_name: &str) -> Result<()> {
    let secret_name = format!("vpn-{}", app_name);
    let namespace = app_name;

    let secrets: Api<Secret> = Api::namespaced(k8s.client().clone(), namespace);

    match secrets.delete(&secret_name, &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(()), // Not found is OK
        Err(e) => Err(AppError::Internal(format!(
            "Failed to delete VPN secret: {}",
            e
        ))),
    }
}

/// Get VPN deployment config for an app (used by deployment service)
pub async fn get_vpn_deployment_config(
    db: &DbConn,
    app_name: &str,
) -> Result<Option<VpnDeploymentConfig>> {
    let config = AppVpnConfig::find_by_id(app_name).one(db).await?;

    if let Some(config) = config {
        let provider = VpnProvider::find_by_id(config.vpn_provider_id)
            .one(db)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!("VPN provider {} not found", config.vpn_provider_id))
            })?;

        if !provider.enabled {
            return Ok(None);
        }

        let kill_switch = config.kill_switch_override.unwrap_or(provider.kill_switch);

        Ok(Some(VpnDeploymentConfig {
            enabled: true,
            secret_name: format!("vpn-{}", app_name),
            kill_switch,
            firewall_outbound_subnets: provider.firewall_outbound_subnets,
        }))
    } else {
        Ok(None)
    }
}

/// VPN deployment configuration for Helm
#[derive(Debug, Clone, Serialize)]
pub struct VpnDeploymentConfig {
    pub enabled: bool,
    pub secret_name: String,
    pub kill_switch: bool,
    pub firewall_outbound_subnets: String,
}

/// VPN connection test result
#[derive(Debug, Clone, Serialize)]
pub struct VpnTestResult {
    pub success: bool,
    pub message: String,
    pub public_ip: Option<String>,
}

// ============================================================================
// VPN Connection Testing
// ============================================================================

/// Test VPN connection by creating a temporary Gluetun pod
pub async fn test_vpn_connection(
    k8s: &K8sClient,
    db: &DbConn,
    provider_id: i64,
) -> Result<VpnTestResult> {
    // Get and validate provider
    let provider = VpnProvider::find_by_id(provider_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("VPN provider {} not found", provider_id)))?;

    if !provider.enabled {
        return Err(AppError::BadRequest(
            "Cannot test a disabled VPN provider".to_string(),
        ));
    }

    // Generate test pod name with random suffix
    let random_suffix: String = (0..8)
        .map(|_| format!("{:x}", rand::random::<u8>() % 16))
        .collect();
    let test_pod_name = format!("vpn-test-{}-{}", provider_id, random_suffix);
    let namespace = "kubarr";

    tracing::info!(
        "Testing VPN provider {} with test pod {}",
        provider.name,
        test_pod_name
    );

    // Create test secret
    let secret_name = create_test_vpn_secret(k8s, db, &test_pod_name, namespace, provider_id).await?;

    // Create test pod
    match create_gluetun_test_pod(k8s, &test_pod_name, namespace, &secret_name).await {
        Ok(_) => {
            tracing::info!("Created test pod {}", test_pod_name);
        }
        Err(e) => {
            // Clean up secret on failure
            let _ = delete_test_vpn_secret(k8s, &secret_name, namespace).await;
            return Err(e);
        }
    }

    // Wait for pod to be ready (this will be implemented in subtask-1-2)
    // For now, just return success
    Ok(VpnTestResult {
        success: true,
        message: format!(
            "Test pod {} created successfully (readiness check pending)",
            test_pod_name
        ),
        public_ip: None,
    })
}

/// Create a K8s secret for VPN test pod
async fn create_test_vpn_secret(
    k8s: &K8sClient,
    db: &DbConn,
    test_pod_name: &str,
    namespace: &str,
    provider_id: i64,
) -> Result<String> {
    // Get provider
    let provider = VpnProvider::find_by_id(provider_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal(format!("VPN provider {} not found", provider_id)))?;

    // Parse credentials
    let credentials: serde_json::Value = serde_json::from_str(&provider.credentials_json)
        .map_err(|e| AppError::Internal(format!("Invalid credentials JSON: {}", e)))?;

    // Build secret data based on VPN type
    let mut secret_data: BTreeMap<String, String> = BTreeMap::new();

    // Set VPN type
    secret_data.insert("VPN_TYPE".to_string(), provider.vpn_type.to_string());

    // Set service provider if specified
    if let Some(ref service_provider) = provider.service_provider {
        if service_provider != "custom" {
            secret_data.insert("VPN_SERVICE_PROVIDER".to_string(), service_provider.clone());
        }
    }

    match provider.vpn_type {
        vpn_provider::VpnType::WireGuard => {
            build_wireguard_secret_data(&credentials, &mut secret_data)?;
        }
        vpn_provider::VpnType::OpenVpn => {
            build_openvpn_secret_data(&credentials, &mut secret_data)?;
        }
    }

    // Convert to base64-encoded data
    let encoded_data: BTreeMap<String, k8s_openapi::ByteString> = secret_data
        .into_iter()
        .map(|(k, v)| (k, k8s_openapi::ByteString(v.into_bytes())))
        .collect();

    let secret_name = format!("vpn-test-{}", test_pod_name);

    // Create secret object
    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(secret_name.clone()),
            namespace: Some(namespace.to_string()),
            labels: Some(BTreeMap::from([
                (
                    "app.kubernetes.io/managed-by".to_string(),
                    "kubarr".to_string(),
                ),
                (
                    "kubarr.io/vpn-test".to_string(),
                    "true".to_string(),
                ),
            ])),
            ..Default::default()
        },
        data: Some(encoded_data),
        ..Default::default()
    };

    // Create in K8s
    let secrets: Api<Secret> = Api::namespaced(k8s.client().clone(), namespace);

    // Try to delete existing secret first (in case of previous failed test)
    let _ = secrets.delete(&secret_name, &DeleteParams::default()).await;

    // Create new secret
    secrets.create(&PostParams::default(), &secret).await?;

    Ok(secret_name)
}

/// Delete a test VPN secret
async fn delete_test_vpn_secret(k8s: &K8sClient, secret_name: &str, namespace: &str) -> Result<()> {
    let secrets: Api<Secret> = Api::namespaced(k8s.client().clone(), namespace);

    match secrets.delete(secret_name, &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(()), // Not found is OK
        Err(e) => {
            tracing::warn!("Failed to delete test VPN secret {}: {}", secret_name, e);
            Ok(()) // Don't fail on cleanup errors
        }
    }
}

/// Create a Gluetun test pod
async fn create_gluetun_test_pod(
    k8s: &K8sClient,
    pod_name: &str,
    namespace: &str,
    secret_name: &str,
) -> Result<()> {
    let pods: Api<Pod> = Api::namespaced(k8s.client().clone(), namespace);

    // Build environment variables from secret
    let env_vars = vec![
        EnvVar {
            name: "HEALTH_SERVER_ADDRESS".to_string(),
            value: Some(":9999".to_string()),
            ..Default::default()
        },
    ];

    // Build pod spec
    let pod = Pod {
        metadata: ObjectMeta {
            name: Some(pod_name.to_string()),
            namespace: Some(namespace.to_string()),
            labels: Some(BTreeMap::from([
                ("app.kubernetes.io/name".to_string(), "gluetun-test".to_string()),
                ("app.kubernetes.io/managed-by".to_string(), "kubarr".to_string()),
                ("kubarr.io/vpn-test".to_string(), "true".to_string()),
            ])),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "gluetun".to_string(),
                image: Some("qmcgaw/gluetun:latest".to_string()),
                env: Some(env_vars),
                env_from: Some(vec![k8s_openapi::api::core::v1::EnvFromSource {
                    secret_ref: Some(k8s_openapi::api::core::v1::SecretEnvSource {
                        name: secret_name.to_string(),
                        optional: Some(false),
                    }),
                    ..Default::default()
                }]),
                security_context: Some(SecurityContext {
                    capabilities: Some(Capabilities {
                        add: Some(vec!["NET_ADMIN".to_string()]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            restart_policy: Some("Never".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Create pod
    pods.create(&PostParams::default(), &pod).await?;

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_credentials(
    vpn_type: &vpn_provider::VpnType,
    credentials: &serde_json::Value,
) -> Result<()> {
    match vpn_type {
        vpn_provider::VpnType::WireGuard => {
            // Must have private_key
            if credentials
                .get("private_key")
                .and_then(|v| v.as_str())
                .is_none()
            {
                return Err(AppError::BadRequest(
                    "WireGuard credentials must include private_key".to_string(),
                ));
            }
        }
        vpn_provider::VpnType::OpenVpn => {
            // Must have username and password
            if credentials
                .get("username")
                .and_then(|v| v.as_str())
                .is_none()
            {
                return Err(AppError::BadRequest(
                    "OpenVPN credentials must include username".to_string(),
                ));
            }
            if credentials
                .get("password")
                .and_then(|v| v.as_str())
                .is_none()
            {
                return Err(AppError::BadRequest(
                    "OpenVPN credentials must include password".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn build_wireguard_secret_data(
    credentials: &serde_json::Value,
    data: &mut BTreeMap<String, String>,
) -> Result<()> {
    // Private key (required)
    if let Some(private_key) = credentials.get("private_key").and_then(|v| v.as_str()) {
        data.insert("WIREGUARD_PRIVATE_KEY".to_string(), private_key.to_string());
    }

    // Addresses
    if let Some(addresses) = credentials.get("addresses").and_then(|v| v.as_array()) {
        let addr_str: Vec<&str> = addresses.iter().filter_map(|v| v.as_str()).collect();
        if !addr_str.is_empty() {
            data.insert("WIREGUARD_ADDRESSES".to_string(), addr_str.join(","));
        }
    }

    // Public key (for custom WireGuard)
    if let Some(public_key) = credentials.get("public_key").and_then(|v| v.as_str()) {
        data.insert("WIREGUARD_PUBLIC_KEY".to_string(), public_key.to_string());
    }

    // Endpoint
    if let Some(endpoint_ip) = credentials.get("endpoint_ip").and_then(|v| v.as_str()) {
        data.insert("VPN_ENDPOINT_IP".to_string(), endpoint_ip.to_string());
    }
    if let Some(endpoint_port) = credentials.get("endpoint_port").and_then(|v| v.as_u64()) {
        data.insert("VPN_ENDPOINT_PORT".to_string(), endpoint_port.to_string());
    }

    // Preshared key
    if let Some(preshared_key) = credentials.get("preshared_key").and_then(|v| v.as_str()) {
        data.insert(
            "WIREGUARD_PRESHARED_KEY".to_string(),
            preshared_key.to_string(),
        );
    }

    Ok(())
}

fn build_openvpn_secret_data(
    credentials: &serde_json::Value,
    data: &mut BTreeMap<String, String>,
) -> Result<()> {
    // Username and password (required)
    if let Some(username) = credentials.get("username").and_then(|v| v.as_str()) {
        data.insert("OPENVPN_USER".to_string(), username.to_string());
    }
    if let Some(password) = credentials.get("password").and_then(|v| v.as_str()) {
        data.insert("OPENVPN_PASSWORD".to_string(), password.to_string());
    }

    // Server selection
    if let Some(countries) = credentials.get("server_countries").and_then(|v| v.as_str()) {
        data.insert("SERVER_COUNTRIES".to_string(), countries.to_string());
    }
    if let Some(cities) = credentials.get("server_cities").and_then(|v| v.as_str()) {
        data.insert("SERVER_CITIES".to_string(), cities.to_string());
    }
    if let Some(hostnames) = credentials.get("server_hostnames").and_then(|v| v.as_str()) {
        data.insert("SERVER_HOSTNAMES".to_string(), hostnames.to_string());
    }

    Ok(())
}

/// Get list of supported VPN service providers
pub fn get_supported_providers() -> Vec<SupportedProvider> {
    vec![
        SupportedProvider {
            id: "custom",
            name: "Custom",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Custom WireGuard or OpenVPN configuration",
        },
        SupportedProvider {
            id: "airvpn",
            name: "AirVPN",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Privacy-focused VPN with WireGuard and OpenVPN",
        },
        SupportedProvider {
            id: "expressvpn",
            name: "ExpressVPN",
            vpn_types: vec!["openvpn"],
            description: "Fast VPN with servers in 94 countries",
        },
        SupportedProvider {
            id: "ipvanish",
            name: "IPVanish",
            vpn_types: vec!["openvpn"],
            description: "VPN with configurable encryption",
        },
        SupportedProvider {
            id: "mullvad",
            name: "Mullvad",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Privacy-first VPN, no email required",
        },
        SupportedProvider {
            id: "nordvpn",
            name: "NordVPN",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Popular VPN with NordLynx (WireGuard)",
        },
        SupportedProvider {
            id: "private_internet_access",
            name: "Private Internet Access (PIA)",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Proven no-logs VPN",
        },
        SupportedProvider {
            id: "protonvpn",
            name: "ProtonVPN",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Swiss-based privacy VPN",
        },
        SupportedProvider {
            id: "surfshark",
            name: "Surfshark",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "Unlimited device connections",
        },
        SupportedProvider {
            id: "windscribe",
            name: "Windscribe",
            vpn_types: vec!["wireguard", "openvpn"],
            description: "VPN with built-in ad blocker",
        },
    ]
}
