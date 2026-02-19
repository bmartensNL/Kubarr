use std::env;

/// Configuration for external monitoring services
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// VictoriaMetrics service URL (env: `KUBARR_VICTORIAMETRICS_URL`)
    pub victoriametrics_url: String,
    /// VictoriaLogs service URL (env: `KUBARR_VICTORIALOGS_URL`)
    pub victorialogs_url: String,
}

impl MonitoringConfig {
    pub fn from_env() -> Self {
        Self {
            victoriametrics_url: env::var("KUBARR_VICTORIAMETRICS_URL").unwrap_or_else(|_| {
                "http://victoriametrics.victoriametrics.svc.cluster.local:8428".to_string()
            }),
            victorialogs_url: env::var("KUBARR_VICTORIALOGS_URL").unwrap_or_else(|_| {
                "http://victorialogs.victorialogs.svc.cluster.local:9428".to_string()
            }),
        }
    }
}
