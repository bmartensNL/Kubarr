use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct KubernetesConfig {
    pub kubeconfig_path: Option<PathBuf>,
    pub in_cluster: bool,
    pub default_namespace: String,
}

impl KubernetesConfig {
    pub fn from_env() -> Self {
        Self {
            kubeconfig_path: env::var("KUBARR_KUBECONFIG_PATH").ok().map(PathBuf::from),
            in_cluster: env::var("KUBARR_IN_CLUSTER")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            default_namespace: env::var("KUBARR_DEFAULT_NAMESPACE")
                .unwrap_or_else(|_| "media".to_string()),
        }
    }
}
