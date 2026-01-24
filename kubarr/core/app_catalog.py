"""Application catalog for Kubarr.

This module dynamically discovers available applications from Helm charts.
"""

import os
from pathlib import Path
from typing import Dict, List, Optional

import yaml

from kubarr.core.models import AppConfig, ResourceRequirements, VolumeConfig

# Path to charts directory
# In container: /app/charts, locally: project_root/charts
CHARTS_DIR = Path(os.environ.get("CHARTS_DIR", "/app/charts"))


class AppCatalog:
    """Registry of all available applications, loaded from Helm charts."""

    def __init__(self, charts_dir: Optional[Path] = None) -> None:
        """Initialize the app catalog.

        Args:
            charts_dir: Path to charts directory (uses CHARTS_DIR env var if not provided)
        """
        self._charts_dir = charts_dir or CHARTS_DIR
        self._apps: Dict[str, AppConfig] = {}
        self._load_apps()

    def _load_apps(self) -> None:
        """Load all app definitions from Helm charts."""
        if not self._charts_dir.exists():
            return

        for chart_dir in self._charts_dir.iterdir():
            if not chart_dir.is_dir():
                continue

            chart_yaml = chart_dir / "Chart.yaml"
            values_yaml = chart_dir / "values.yaml"

            if not chart_yaml.exists():
                continue

            try:
                app_config = self._parse_chart(chart_dir.name, chart_yaml, values_yaml)
                if app_config:
                    self._apps[app_config.name] = app_config
            except Exception as e:
                # Log error but continue loading other charts
                print(f"Warning: Failed to load chart {chart_dir.name}: {e}")

    def _parse_chart(
        self,
        chart_name: str,
        chart_yaml: Path,
        values_yaml: Path
    ) -> Optional[AppConfig]:
        """Parse a Helm chart into an AppConfig.

        Args:
            chart_name: Name of the chart directory
            chart_yaml: Path to Chart.yaml
            values_yaml: Path to values.yaml

        Returns:
            AppConfig or None if chart doesn't have kubarr annotations
        """
        with open(chart_yaml, "r", encoding="utf-8") as f:
            chart = yaml.safe_load(f)

        # Get kubarr annotations - skip charts without them
        annotations = chart.get("annotations", {})
        if not annotations.get("kubarr.io/category"):
            return None

        # Parse values.yaml for image, port, and resources
        values = {}
        if values_yaml.exists():
            with open(values_yaml, "r", encoding="utf-8") as f:
                values = yaml.safe_load(f) or {}

        # Get app-specific config (nested under app name)
        app_values = values.get(chart_name, {})

        # Extract image info
        image_config = app_values.get("image", {})
        image_repo = image_config.get("repository", f"linuxserver/{chart_name}")
        image_tag = image_config.get("tag", "latest")
        container_image = f"{image_repo}:{image_tag}"

        # Extract port
        service_config = app_values.get("service", {})
        default_port = service_config.get("port", 8080)

        # Extract resources
        resources_config = app_values.get("resources", {})
        requests = resources_config.get("requests", {})
        limits = resources_config.get("limits", {})

        resource_requirements = ResourceRequirements(
            cpu_request=requests.get("cpu", "100m"),
            cpu_limit=limits.get("cpu", "1000m"),
            memory_request=requests.get("memory", "256Mi"),
            memory_limit=limits.get("memory", "1Gi"),
        )

        # Extract volumes from persistence config
        volumes = []
        persistence = values.get("persistence", {})
        for vol_name, vol_config in persistence.items():
            if isinstance(vol_config, dict) and vol_config.get("enabled", True):
                volumes.append(VolumeConfig(
                    name=vol_name,
                    mount_path=vol_config.get("mountPath", f"/{vol_name}"),
                    size=vol_config.get("size", "1Gi"),
                ))

        # Extract environment variables
        env_vars = app_values.get("env", {})

        # Check if this is a system app
        is_system = annotations.get("kubarr.io/system", "false").lower() == "true"
        # Check if this app should be hidden (no Open button)
        is_hidden = annotations.get("kubarr.io/hidden", "false").lower() == "true"

        return AppConfig(
            name=chart_name,
            display_name=annotations.get("kubarr.io/display-name", chart_name.title()),
            description=chart.get("description", ""),
            icon=annotations.get("kubarr.io/icon", "📦"),
            container_image=container_image,
            default_port=default_port,
            resource_requirements=resource_requirements,
            volumes=volumes,
            environment_variables=env_vars,
            category=annotations.get("kubarr.io/category", "other"),
            is_system=is_system,
            is_hidden=is_hidden,
        )

    def get_all_apps(self) -> List[AppConfig]:
        """Get all available apps.

        Returns:
            List of all AppConfig instances
        """
        return list(self._apps.values())

    def get_app(self, app_name: str) -> Optional[AppConfig]:
        """Get a specific app by name.

        Args:
            app_name: Name of the app to retrieve

        Returns:
            AppConfig instance or None if not found
        """
        return self._apps.get(app_name.lower())

    def get_apps_by_category(self, category: str) -> List[AppConfig]:
        """Get all apps in a specific category.

        Args:
            category: Category to filter by

        Returns:
            List of AppConfig instances in the category
        """
        return [app for app in self._apps.values() if app.category == category]

    def app_exists(self, app_name: str) -> bool:
        """Check if an app exists in the catalog.

        Args:
            app_name: Name of the app to check

        Returns:
            True if app exists
        """
        return app_name.lower() in self._apps

    def get_categories(self) -> List[str]:
        """Get all unique categories.

        Returns:
            List of category names
        """
        return list(set(app.category for app in self._apps.values()))

    def reload(self) -> None:
        """Reload apps from charts directory."""
        self._apps.clear()
        self._load_apps()
