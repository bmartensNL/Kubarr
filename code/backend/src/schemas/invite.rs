use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvite {
    #[serde(default = "default_invite_days")]
    pub expires_in_days: i32,
}

fn default_invite_days() -> i32 {
    7
}

#[derive(Debug, Clone, Serialize)]
pub struct InviteResponse {
    pub id: i64,
    pub code: String,
    pub created_by_id: i64,
    pub created_by_username: String,
    pub used_by_id: Option<i64>,
    pub used_by_username: Option<String>,
    pub is_used: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}
