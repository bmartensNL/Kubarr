use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "cloudflare_tunnels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    /// Tunnel token (from CF API or user-provided) — never serialised in API responses
    #[serde(skip_serializing)]
    pub tunnel_token: String,
    /// Deployment status: not_deployed | deploying | running | failed | removing
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    // ── Cloudflare API fields (populated by the guided wizard) ───────────────
    /// CF API token — never serialised in API responses
    #[serde(skip_serializing)]
    pub api_token: Option<String>,
    /// CF account ID
    pub account_id: Option<String>,
    /// CF tunnel UUID
    pub tunnel_id: Option<String>,
    /// CF zone ID
    pub zone_id: Option<String>,
    /// Zone name, e.g. "example.com"
    pub zone_name: Option<String>,
    /// Subdomain entered by the user, e.g. "kubarr"
    pub subdomain: Option<String>,
    /// CF DNS record ID (used for cleanup)
    pub dns_record_id: Option<String>,
    /// Full hostname, e.g. "kubarr.example.com"
    pub hostname: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
