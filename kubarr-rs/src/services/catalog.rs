use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::CONFIG;
use crate::error::Result;

/// App configuration from Helm chart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub icon: String,
    pub container_image: String,
    pub default_port: i32,
    pub resource_requirements: ResourceRequirements,
    pub volumes: Vec<VolumeConfig>,
    pub environment_variables: HashMap<String, String>,
    pub category: String,
    pub is_system: bool,
    pub is_hidden: bool,
    pub is_browseable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu_request: String,
    pub cpu_limit: String,
    pub memory_request: String,
    pub memory_limit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub mount_path: String,
    pub size: String,
}

/// App catalog - registry of all available applications
pub struct AppCatalog {
    apps: HashMap<String, AppConfig>,
}

impl AppCatalog {
    /// Create a new app catalog from charts directory
    pub fn new() -> Self {
        let mut catalog = Self {
            apps: HashMap::new(),
        };
        catalog.load_apps();
        catalog
    }

    /// Load all app definitions from Helm charts
    fn load_apps(&mut self) {
        let charts_dir = &CONFIG.charts_dir;

        if !charts_dir.exists() {
            tracing::warn!("Charts directory does not exist: {}", charts_dir.display());
            return;
        }

        if let Ok(entries) = std::fs::read_dir(charts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(chart_name) = path.file_name().and_then(|n| n.to_str()) {
                        match self.parse_chart(chart_name, &path) {
                            Ok(Some(app)) => {
                                self.apps.insert(app.name.clone(), app);
                            }
                            Ok(None) => {
                                // Chart doesn't have kubarr annotations, skip
                            }
                            Err(e) => {
                                tracing::warn!("Failed to load chart {}: {}", chart_name, e);
                            }
                        }
                    }
                }
            }
        }

        tracing::info!("Loaded {} apps from catalog", self.apps.len());
    }

    /// Parse a Helm chart into an AppConfig
    fn parse_chart(&self, chart_name: &str, chart_dir: &Path) -> Result<Option<AppConfig>> {
        let chart_yaml = chart_dir.join("Chart.yaml");
        let values_yaml = chart_dir.join("values.yaml");

        if !chart_yaml.exists() {
            return Ok(None);
        }

        // Parse Chart.yaml
        let chart_content = std::fs::read_to_string(&chart_yaml)?;
        let chart: serde_yaml::Value = serde_yaml::from_str(&chart_content)?;

        // Get kubarr annotations
        let annotations = chart
            .get("annotations")
            .and_then(|a| a.as_mapping())
            .cloned()
            .unwrap_or_default();

        // Skip charts without kubarr category annotation
        let category =
            match annotations.get(&serde_yaml::Value::String("kubarr.io/category".to_string())) {
                Some(c) => c.as_str().unwrap_or("other").to_string(),
                None => return Ok(None),
            };

        // Parse values.yaml
        let values: serde_yaml::Value = if values_yaml.exists() {
            let values_content = std::fs::read_to_string(&values_yaml)?;
            serde_yaml::from_str(&values_content).unwrap_or(serde_yaml::Value::Null)
        } else {
            serde_yaml::Value::Null
        };

        // Get app-specific config
        let app_values = values
            .get(chart_name)
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);

        // Extract image info
        let image_config = app_values
            .get("image")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);
        let default_image_repo = format!("linuxserver/{}", chart_name);
        let image_repo = image_config
            .get("repository")
            .and_then(|r| r.as_str())
            .unwrap_or(&default_image_repo);
        let image_tag = image_config
            .get("tag")
            .and_then(|t| t.as_str())
            .unwrap_or("latest");
        let container_image = format!("{}:{}", image_repo, image_tag);

        // Extract port
        let service_config = app_values
            .get("service")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);
        let default_port = service_config
            .get("port")
            .and_then(|p| p.as_i64())
            .unwrap_or(8080) as i32;

        // Extract resources
        let resources_config = app_values
            .get("resources")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);
        let requests = resources_config
            .get("requests")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);
        let limits = resources_config
            .get("limits")
            .cloned()
            .unwrap_or(serde_yaml::Value::Null);

        let resource_requirements = ResourceRequirements {
            cpu_request: requests
                .get("cpu")
                .and_then(|c| c.as_str())
                .unwrap_or("100m")
                .to_string(),
            cpu_limit: limits
                .get("cpu")
                .and_then(|c| c.as_str())
                .unwrap_or("1000m")
                .to_string(),
            memory_request: requests
                .get("memory")
                .and_then(|m| m.as_str())
                .unwrap_or("256Mi")
                .to_string(),
            memory_limit: limits
                .get("memory")
                .and_then(|m| m.as_str())
                .unwrap_or("1Gi")
                .to_string(),
        };

        // Extract volumes
        let mut volumes = Vec::new();
        if let Some(persistence) = values.get("persistence").and_then(|p| p.as_mapping()) {
            for (vol_name, vol_config) in persistence {
                if let (Some(name), Some(config)) = (vol_name.as_str(), vol_config.as_mapping()) {
                    let enabled = config
                        .get(&serde_yaml::Value::String("enabled".to_string()))
                        .and_then(|e| e.as_bool())
                        .unwrap_or(true);

                    if enabled {
                        volumes.push(VolumeConfig {
                            name: name.to_string(),
                            mount_path: config
                                .get(&serde_yaml::Value::String("mountPath".to_string()))
                                .and_then(|m| m.as_str())
                                .unwrap_or(&format!("/{}", name))
                                .to_string(),
                            size: config
                                .get(&serde_yaml::Value::String("size".to_string()))
                                .and_then(|s| s.as_str())
                                .unwrap_or("1Gi")
                                .to_string(),
                        });
                    }
                }
            }
        }

        // Extract environment variables
        let env_vars: HashMap<String, String> = app_values
            .get("env")
            .and_then(|e| e.as_mapping())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        // Get display name and other annotations
        let display_name = annotations
            .get(&serde_yaml::Value::String(
                "kubarr.io/display-name".to_string(),
            ))
            .and_then(|d| d.as_str())
            .unwrap_or(chart_name)
            .to_string();

        let icon = annotations
            .get(&serde_yaml::Value::String("kubarr.io/icon".to_string()))
            .and_then(|i| i.as_str())
            .unwrap_or("📦")
            .to_string();

        let is_system = annotations
            .get(&serde_yaml::Value::String("kubarr.io/system".to_string()))
            .and_then(|s| s.as_str())
            .map(|s| s.to_lowercase() == "true")
            .unwrap_or(false);

        let is_hidden = annotations
            .get(&serde_yaml::Value::String("kubarr.io/hidden".to_string()))
            .and_then(|h| h.as_str())
            .map(|h| h.to_lowercase() == "true")
            .unwrap_or(false);

        // Apps are browseable by default unless explicitly set to false
        let is_browseable = annotations
            .get(&serde_yaml::Value::String("kubarr.io/browseable".to_string()))
            .and_then(|b| b.as_str())
            .map(|b| b.to_lowercase() != "false")
            .unwrap_or(true);

        let description = chart
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        Ok(Some(AppConfig {
            name: chart_name.to_string(),
            display_name,
            description,
            icon,
            container_image,
            default_port,
            resource_requirements,
            volumes,
            environment_variables: env_vars,
            category,
            is_system,
            is_hidden,
            is_browseable,
        }))
    }

    /// Get all available apps
    pub fn get_all_apps(&self) -> Vec<&AppConfig> {
        self.apps.values().collect()
    }

    /// Get a specific app by name
    pub fn get_app(&self, app_name: &str) -> Option<&AppConfig> {
        self.apps.get(&app_name.to_lowercase())
    }

    /// Get all apps in a specific category
    pub fn get_apps_by_category(&self, category: &str) -> Vec<&AppConfig> {
        self.apps
            .values()
            .filter(|app| app.category == category)
            .collect()
    }

    /// Check if an app exists in the catalog
    pub fn app_exists(&self, app_name: &str) -> bool {
        self.apps.contains_key(&app_name.to_lowercase())
    }

    /// Get all unique categories
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories: Vec<_> = self
            .apps
            .values()
            .map(|app| app.category.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        categories.sort();
        categories
    }

    /// Reload apps from charts directory
    pub fn reload(&mut self) {
        self.apps.clear();
        self.load_apps();
    }
}

impl Default for AppCatalog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a minimal Chart.yaml with kubarr annotations
    fn create_test_chart_yaml(dir: &Path, name: &str, category: &str) {
        let chart_content = format!(
            r#"
apiVersion: v2
name: {}
version: 1.0.0
description: Test app for {}
annotations:
  kubarr.io/category: "{}"
  kubarr.io/display-name: "Test {}"
  kubarr.io/icon: "📦"
"#,
            name, name, category, name
        );
        fs::write(dir.join("Chart.yaml"), chart_content).unwrap();
    }

    /// Create a minimal values.yaml
    fn create_test_values_yaml(dir: &Path, name: &str) {
        let values_content = format!(
            r#"
{}:
  image:
    repository: "linuxserver/{}"
    tag: "latest"
  service:
    port: 8080
  resources:
    requests:
      cpu: "100m"
      memory: "256Mi"
    limits:
      cpu: "1000m"
      memory: "1Gi"
persistence:
  config:
    enabled: true
    mountPath: /config
    size: 1Gi
"#,
            name, name
        );
        fs::write(dir.join("values.yaml"), values_content).unwrap();
    }

    #[test]
    fn test_app_config_serialization() {
        let config = AppConfig {
            name: "testapp".to_string(),
            display_name: "Test App".to_string(),
            description: "A test application".to_string(),
            icon: "📦".to_string(),
            container_image: "linuxserver/testapp:latest".to_string(),
            default_port: 8080,
            resource_requirements: ResourceRequirements {
                cpu_request: "100m".to_string(),
                cpu_limit: "1000m".to_string(),
                memory_request: "256Mi".to_string(),
                memory_limit: "1Gi".to_string(),
            },
            volumes: vec![VolumeConfig {
                name: "config".to_string(),
                mount_path: "/config".to_string(),
                size: "1Gi".to_string(),
            }],
            environment_variables: HashMap::new(),
            category: "media".to_string(),
            is_system: false,
            is_hidden: false,
            is_browseable: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"name\":\"testapp\""));
        assert!(json.contains("\"display_name\":\"Test App\""));
        assert!(json.contains("\"category\":\"media\""));

        // Deserialize back
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, config.name);
        assert_eq!(parsed.default_port, config.default_port);
    }

    #[test]
    fn test_resource_requirements_serialization() {
        let resources = ResourceRequirements {
            cpu_request: "100m".to_string(),
            cpu_limit: "500m".to_string(),
            memory_request: "128Mi".to_string(),
            memory_limit: "512Mi".to_string(),
        };

        let json = serde_json::to_string(&resources).unwrap();
        assert!(json.contains("\"cpu_request\":\"100m\""));
        assert!(json.contains("\"memory_limit\":\"512Mi\""));
    }

    #[test]
    fn test_volume_config_serialization() {
        let volume = VolumeConfig {
            name: "data".to_string(),
            mount_path: "/data".to_string(),
            size: "10Gi".to_string(),
        };

        let json = serde_json::to_string(&volume).unwrap();
        assert!(json.contains("\"name\":\"data\""));
        assert!(json.contains("\"mount_path\":\"/data\""));
        assert!(json.contains("\"size\":\"10Gi\""));
    }

    #[test]
    fn test_app_catalog_empty() {
        // Create empty temp directory
        let temp_dir = TempDir::new().unwrap();

        // Create catalog with empty directory
        let catalog = AppCatalog {
            apps: HashMap::new(),
        };

        assert!(catalog.get_all_apps().is_empty());
        assert!(catalog.get_app("nonexistent").is_none());
        assert!(catalog.get_categories().is_empty());
    }

    #[test]
    fn test_app_catalog_get_app() {
        let mut apps = HashMap::new();
        apps.insert(
            "testapp".to_string(),
            AppConfig {
                name: "testapp".to_string(),
                display_name: "Test App".to_string(),
                description: "Test".to_string(),
                icon: "📦".to_string(),
                container_image: "test:latest".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "media".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        let catalog = AppCatalog { apps };

        assert!(catalog.get_app("testapp").is_some());
        assert!(catalog.get_app("TESTAPP").is_some()); // Case insensitive
        assert!(catalog.get_app("nonexistent").is_none());
    }

    #[test]
    fn test_app_catalog_get_apps_by_category() {
        let mut apps = HashMap::new();

        // Add media app
        apps.insert(
            "sonarr".to_string(),
            AppConfig {
                name: "sonarr".to_string(),
                display_name: "Sonarr".to_string(),
                description: "TV series manager".to_string(),
                icon: "📺".to_string(),
                container_image: "linuxserver/sonarr:latest".to_string(),
                default_port: 8989,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "media".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        // Add download app
        apps.insert(
            "qbittorrent".to_string(),
            AppConfig {
                name: "qbittorrent".to_string(),
                display_name: "qBittorrent".to_string(),
                description: "BitTorrent client".to_string(),
                icon: "⬇️".to_string(),
                container_image: "linuxserver/qbittorrent:latest".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "download".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        let catalog = AppCatalog { apps };

        let media_apps = catalog.get_apps_by_category("media");
        assert_eq!(media_apps.len(), 1);
        assert_eq!(media_apps[0].name, "sonarr");

        let download_apps = catalog.get_apps_by_category("download");
        assert_eq!(download_apps.len(), 1);
        assert_eq!(download_apps[0].name, "qbittorrent");

        let other_apps = catalog.get_apps_by_category("other");
        assert!(other_apps.is_empty());
    }

    #[test]
    fn test_app_catalog_get_categories() {
        let mut apps = HashMap::new();

        apps.insert(
            "app1".to_string(),
            AppConfig {
                name: "app1".to_string(),
                display_name: "App 1".to_string(),
                description: "".to_string(),
                icon: "".to_string(),
                container_image: "".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "media".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        apps.insert(
            "app2".to_string(),
            AppConfig {
                name: "app2".to_string(),
                display_name: "App 2".to_string(),
                description: "".to_string(),
                icon: "".to_string(),
                container_image: "".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "download".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        apps.insert(
            "app3".to_string(),
            AppConfig {
                name: "app3".to_string(),
                display_name: "App 3".to_string(),
                description: "".to_string(),
                icon: "".to_string(),
                container_image: "".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "media".to_string(), // Duplicate category
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        let catalog = AppCatalog { apps };

        let categories = catalog.get_categories();
        assert_eq!(categories.len(), 2);
        assert!(categories.contains(&"media".to_string()));
        assert!(categories.contains(&"download".to_string()));
    }

    #[test]
    fn test_app_catalog_app_exists() {
        let mut apps = HashMap::new();
        apps.insert(
            "myapp".to_string(),
            AppConfig {
                name: "myapp".to_string(),
                display_name: "My App".to_string(),
                description: "".to_string(),
                icon: "".to_string(),
                container_image: "".to_string(),
                default_port: 8080,
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    cpu_limit: "1000m".to_string(),
                    memory_request: "256Mi".to_string(),
                    memory_limit: "1Gi".to_string(),
                },
                volumes: vec![],
                environment_variables: HashMap::new(),
                category: "test".to_string(),
                is_system: false,
                is_hidden: false,
                is_browseable: true,
            },
        );

        let catalog = AppCatalog { apps };

        assert!(catalog.app_exists("myapp"));
        assert!(catalog.app_exists("MYAPP")); // Case insensitive
        assert!(!catalog.app_exists("other"));
    }

    #[test]
    fn test_app_config_clone() {
        let config = AppConfig {
            name: "testapp".to_string(),
            display_name: "Test App".to_string(),
            description: "Description".to_string(),
            icon: "📦".to_string(),
            container_image: "test:latest".to_string(),
            default_port: 8080,
            resource_requirements: ResourceRequirements {
                cpu_request: "100m".to_string(),
                cpu_limit: "1000m".to_string(),
                memory_request: "256Mi".to_string(),
                memory_limit: "1Gi".to_string(),
            },
            volumes: vec![VolumeConfig {
                name: "config".to_string(),
                mount_path: "/config".to_string(),
                size: "1Gi".to_string(),
            }],
            environment_variables: {
                let mut map = HashMap::new();
                map.insert("KEY".to_string(), "VALUE".to_string());
                map
            },
            category: "media".to_string(),
            is_system: false,
            is_hidden: false,
            is_browseable: true,
        };

        let cloned = config.clone();

        assert_eq!(config.name, cloned.name);
        assert_eq!(config.display_name, cloned.display_name);
        assert_eq!(config.default_port, cloned.default_port);
        assert_eq!(config.volumes.len(), cloned.volumes.len());
        assert_eq!(config.environment_variables, cloned.environment_variables);
    }

    #[test]
    fn test_system_and_hidden_flags() {
        let system_app = AppConfig {
            name: "system".to_string(),
            display_name: "System".to_string(),
            description: "".to_string(),
            icon: "".to_string(),
            container_image: "".to_string(),
            default_port: 8080,
            resource_requirements: ResourceRequirements {
                cpu_request: "100m".to_string(),
                cpu_limit: "1000m".to_string(),
                memory_request: "256Mi".to_string(),
                memory_limit: "1Gi".to_string(),
            },
            volumes: vec![],
            environment_variables: HashMap::new(),
            category: "system".to_string(),
            is_system: true,
            is_hidden: false,
            is_browseable: true,
        };

        let hidden_app = AppConfig {
            name: "hidden".to_string(),
            display_name: "Hidden".to_string(),
            description: "".to_string(),
            icon: "".to_string(),
            container_image: "".to_string(),
            default_port: 8080,
            resource_requirements: ResourceRequirements {
                cpu_request: "100m".to_string(),
                cpu_limit: "1000m".to_string(),
                memory_request: "256Mi".to_string(),
                memory_limit: "1Gi".to_string(),
            },
            volumes: vec![],
            environment_variables: HashMap::new(),
            category: "other".to_string(),
            is_system: false,
            is_hidden: true,
            is_browseable: true,
        };

        assert!(system_app.is_system);
        assert!(!system_app.is_hidden);
        assert!(!hidden_app.is_system);
        assert!(hidden_app.is_hidden);
    }
}
