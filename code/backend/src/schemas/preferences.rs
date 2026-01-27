use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct UserPreferencesResponse {
    pub theme: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserPreferences {
    pub theme: Option<String>,
}
