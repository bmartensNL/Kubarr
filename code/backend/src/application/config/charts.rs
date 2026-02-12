use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ChartsConfig {
    pub dir: PathBuf,
    pub repo: String,
    pub registry: String,
    pub sync_interval: u64,
    pub git_ref: String,
}

impl ChartsConfig {
    pub fn from_env() -> Self {
        Self {
            dir: PathBuf::from(
                env::var("KUBARR_CHARTS_DIR").unwrap_or_else(|_| "/app/charts".to_string()),
            ),
            repo: env::var("KUBARR_CHARTS_REPO")
                .unwrap_or_else(|_| "bmartensNL/kubarr-charts".to_string()),
            registry: env::var("KUBARR_CHARTS_REGISTRY")
                .unwrap_or_else(|_| "oci://ghcr.io/bmartensnl/kubarr/charts".to_string()),
            sync_interval: env::var("KUBARR_CHARTS_SYNC_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
            git_ref: env::var("KUBARR_CHARTS_GIT_REF")
                .unwrap_or_else(|_| "main".to_string()),
        }
    }
}
