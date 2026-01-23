"""Application catalog for Kubarr.

This module defines all available applications that can be deployed through Kubarr.
"""

from typing import Dict, List, Optional

from kubarr.core.models import AppConfig, ResourceRequirements, VolumeConfig


class AppCatalog:
    """Registry of all available applications."""

    def __init__(self) -> None:
        """Initialize the app catalog."""
        self._apps: Dict[str, AppConfig] = self._load_apps()

    def _load_apps(self) -> Dict[str, AppConfig]:
        """Load all app definitions.

        Returns:
            Dictionary mapping app names to AppConfig instances
        """
        apps = [
            # Radarr - Movie collection manager
            AppConfig(
                name="radarr",
                display_name="Radarr",
                description="Movie collection manager for Usenet and BitTorrent users",
                icon="\U0001F3AC",  # 🎬
                container_image="linuxserver/radarr:latest",
                default_port=7878,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="1000m",
                    memory_request="256Mi",
                    memory_limit="1Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="movies", mount_path="/movies", size="100Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="media-manager"
            ),

            # Sonarr - TV series collection manager
            AppConfig(
                name="sonarr",
                display_name="Sonarr",
                description="TV series collection manager for Usenet and BitTorrent users",
                icon="\U0001F4FA",  # 📺
                container_image="linuxserver/sonarr:latest",
                default_port=8989,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="1000m",
                    memory_request="256Mi",
                    memory_limit="1Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="tv", mount_path="/tv", size="100Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="media-manager"
            ),

            # qBittorrent - BitTorrent client
            AppConfig(
                name="qbittorrent",
                display_name="qBittorrent",
                description="Free and open-source BitTorrent client with web interface",
                icon="\U0001F4E5",  # 📥
                container_image="linuxserver/qbittorrent:latest",
                default_port=8080,
                resource_requirements=ResourceRequirements(
                    cpu_request="200m",
                    cpu_limit="2000m",
                    memory_request="512Mi",
                    memory_limit="2Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="downloads", mount_path="/downloads", size="200Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                    "WEBUI_PORT": "8080",
                },
                category="download-client"
            ),

            # Jellyseerr - Media request and discovery tool
            AppConfig(
                name="jellyseerr",
                display_name="Jellyseerr",
                description="Request management and media discovery tool for Jellyfin",
                icon="\U0001F50D",  # 🔍
                container_image="fallenbagel/jellyseerr:latest",
                default_port=5055,
                resource_requirements=ResourceRequirements(
                    cpu_request="50m",
                    cpu_limit="500m",
                    memory_request="128Mi",
                    memory_limit="512Mi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/app/config", size="1Gi"),
                ],
                environment_variables={
                    "LOG_LEVEL": "info",
                    "TZ": "Etc/UTC",
                },
                category="request-manager"
            ),

            # Jellyfin - Media server
            AppConfig(
                name="jellyfin",
                display_name="Jellyfin",
                description="Free software media server for organizing and streaming media",
                icon="\U0001F4FA",  # 📺
                container_image="linuxserver/jellyfin:latest",
                default_port=8096,
                resource_requirements=ResourceRequirements(
                    cpu_request="200m",
                    cpu_limit="4000m",
                    memory_request="512Mi",
                    memory_limit="4Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="5Gi"),
                    VolumeConfig(name="cache", mount_path="/cache", size="10Gi"),
                    VolumeConfig(name="media", mount_path="/media", size="500Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="media-server"
            ),

            # Jackett - Indexer proxy
            AppConfig(
                name="jackett",
                display_name="Jackett",
                description="API support for torrent trackers for Sonarr and Radarr",
                icon="\U0001F50D",  # 🔍
                container_image="linuxserver/jackett:latest",
                default_port=9117,
                resource_requirements=ResourceRequirements(
                    cpu_request="50m",
                    cpu_limit="500m",
                    memory_request="128Mi",
                    memory_limit="512Mi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="indexer"
            ),

            # SABnzbd - Usenet binary newsreader
            AppConfig(
                name="sabnzbd",
                display_name="SABnzbd",
                description="Free and open-source binary newsreader with web interface",
                icon="\U0001F4E5",  # 📥
                container_image="linuxserver/sabnzbd:latest",
                default_port=8080,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="2000m",
                    memory_request="256Mi",
                    memory_limit="2Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="downloads", mount_path="/downloads", size="200Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="download-client"
            ),

            # Transmission - BitTorrent client
            AppConfig(
                name="transmission",
                display_name="Transmission",
                description="Fast, easy, and free BitTorrent client with web interface",
                icon="\U0001F4E5",  # 📥
                container_image="linuxserver/transmission:latest",
                default_port=9091,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="1000m",
                    memory_request="256Mi",
                    memory_limit="1Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="downloads", mount_path="/downloads", size="200Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="download-client"
            ),

            # Deluge - BitTorrent client
            AppConfig(
                name="deluge",
                display_name="Deluge",
                description="Lightweight, cross-platform BitTorrent client with web interface",
                icon="\U0001F4E5",  # 📥
                container_image="linuxserver/deluge:latest",
                default_port=8112,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="1000m",
                    memory_request="256Mi",
                    memory_limit="1Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="downloads", mount_path="/downloads", size="200Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="download-client"
            ),

            # ruTorrent - BitTorrent client with web UI
            AppConfig(
                name="rutorrent",
                display_name="ruTorrent",
                description="Feature-rich BitTorrent client with powerful web interface",
                icon="\U0001F4E5",  # 📥
                container_image="crazymax/rtorrent-rutorrent:latest",
                default_port=8080,
                resource_requirements=ResourceRequirements(
                    cpu_request="100m",
                    cpu_limit="1000m",
                    memory_request="256Mi",
                    memory_limit="1Gi"
                ),
                volumes=[
                    VolumeConfig(name="config", mount_path="/config", size="1Gi"),
                    VolumeConfig(name="downloads", mount_path="/downloads", size="200Gi"),
                ],
                environment_variables={
                    "PUID": "1000",
                    "PGID": "1000",
                    "TZ": "Etc/UTC",
                },
                category="download-client"
            ),
        ]

        return {app.name: app for app in apps}

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
