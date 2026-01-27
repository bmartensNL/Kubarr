use std::collections::HashMap;

use kubarr::services::catalog::{AppCatalog, AppConfig, ResourceRequirements, VolumeConfig};

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
    let catalog = AppCatalog::with_apps(HashMap::new());

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

    let catalog = AppCatalog::with_apps(apps);

    assert!(catalog.get_app("testapp").is_some());
    assert!(catalog.get_app("TESTAPP").is_some()); // Case insensitive
    assert!(catalog.get_app("nonexistent").is_none());
}

#[test]
fn test_app_catalog_get_apps_by_category() {
    let mut apps = HashMap::new();

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

    let catalog = AppCatalog::with_apps(apps);

    let media_apps = catalog.get_apps_by_category("media");
    assert_eq!(media_apps.len(), 1);
    assert_eq!(media_apps[0].name, "sonarr");

    let download_apps = catalog.get_apps_by_category("download");
    assert_eq!(download_apps.len(), 1);
    assert_eq!(download_apps[0].name, "qbittorrent");

    let other_apps = catalog.get_apps_by_category("other");
    assert!(other_apps.is_empty());
}
