"""Configuration constants for Kubarr deployment."""

from pathlib import Path

# Default values
DEFAULT_CLUSTER_NAME = "kubarr-test"
DEFAULT_HOST_PORT = 8080
DEFAULT_ADMIN_EMAIL = "admin@example.com"
DEFAULT_ADMIN_PASSWORD = "admin123"

# Chart deployment order (kubarr must be first - provides OAuth2)
CORE_CHARTS = ["kubarr", "nginx", "oauth2-proxy"]
MONITORING_CHARTS = ["prometheus", "loki", "promtail", "grafana"]

# All available charts
ALL_CHARTS = CORE_CHARTS + MONITORING_CHARTS

# Namespaces (chart name = namespace)
NAMESPACES = {
    "kubarr": "kubarr",
    "nginx": "nginx",
    "oauth2-proxy": "oauth2-proxy",
    "prometheus": "prometheus",
    "loki": "loki",
    "promtail": "promtail",
    "grafana": "grafana",
}

# Components that can be built/redeployed
COMPONENTS = ["frontend", "backend"]

# Docker image names
IMAGES = {
    "frontend": "kubarr-frontend",
    "backend": "kubarr-backend",
}

# Dockerfiles
DOCKERFILES = {
    "frontend": "docker/Dockerfile.frontend",
    "backend": "docker/Dockerfile.backend",
}

# Deployment names for each component
DEPLOYMENTS = {
    "frontend": "kubarr-frontend",
    "backend": "kubarr",
}

# Container names within deployments
CONTAINERS = {
    "frontend": "frontend",
    "backend": "backend",
}

# Timeouts (seconds)
TIMEOUT_BUILD = 600
TIMEOUT_DEPLOY = 300
TIMEOUT_POD_READY = 120


def get_project_root() -> Path:
    """Get the project root directory."""
    # This file is at scripts/kubarr/config.py
    # Project root is two levels up
    return Path(__file__).resolve().parent.parent.parent


def get_charts_dir() -> Path:
    """Get the charts directory."""
    return get_project_root() / "charts"
