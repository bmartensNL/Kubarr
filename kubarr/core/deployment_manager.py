"""Deployment manager for Kubarr applications."""

import os
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional

from kubernetes import client
from kubernetes.client.rest import ApiException

from kubarr.core.app_catalog import AppCatalog
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppConfig, DeploymentRequest, DeploymentStatus

# Path to charts directory
# In container: /app/charts, locally: project_root/charts
CHARTS_DIR = Path(os.environ.get("CHARTS_DIR", "/app/charts"))


class DeploymentManager:
    """Manages deployment, removal, and updates of applications."""

    def __init__(
        self,
        k8s_client: K8sClientManager,
        catalog: Optional[AppCatalog] = None
    ) -> None:
        """Initialize the deployment manager.

        Args:
            k8s_client: Kubernetes client manager
            catalog: App catalog (creates new one if not provided)
        """
        self._k8s = k8s_client
        self._catalog = catalog or AppCatalog()

    def _get_chart_path(self, app_name: str) -> Optional[Path]:
        """Get the path to a Helm chart for an app.

        Args:
            app_name: Name of the app

        Returns:
            Path to chart directory if it exists, None otherwise
        """
        chart_path = CHARTS_DIR / app_name
        if chart_path.exists() and (chart_path / "Chart.yaml").exists():
            return chart_path
        return None

    def _run_helm_command(self, args: List[str]) -> subprocess.CompletedProcess:
        """Run a Helm command.

        Args:
            args: Command arguments (without 'helm' prefix)

        Returns:
            CompletedProcess result

        Raises:
            RuntimeError: If command fails
        """
        cmd = ["helm"] + args
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            raise RuntimeError(f"Helm command failed: {result.stderr}")
        return result

    def deploy_app(
        self,
        request: DeploymentRequest,
        dry_run: bool = False
    ) -> DeploymentStatus:
        """Deploy an application to Kubernetes using Helm.

        Args:
            request: Deployment request with app name and config
            dry_run: If True, validate but don't actually deploy

        Returns:
            DeploymentStatus with result

        Raises:
            ValueError: If app not found in catalog or no chart exists
            RuntimeError: If deployment fails
        """
        # Get app config from catalog
        app_config = self._catalog.get_app(request.app_name)
        if not app_config:
            raise ValueError(f"App '{request.app_name}' not found in catalog")

        # Check if Helm chart exists for this app
        chart_path = self._get_chart_path(request.app_name)
        if not chart_path:
            raise ValueError(f"No Helm chart found for app '{request.app_name}'")

        # Use app name as namespace
        namespace = request.app_name

        try:
            # Build helm install command
            helm_args = [
                "install" if not dry_run else "template",
                request.app_name,
                str(chart_path),
                "-n", namespace,
            ]

            # Only add --create-namespace for actual install
            if not dry_run:
                helm_args.append("--create-namespace")

            # Add any custom config as --set arguments
            if request.custom_config:
                for key, value in request.custom_config.items():
                    helm_args.extend(["--set", f"{key}={value}"])

            self._run_helm_command(helm_args)

            return DeploymentStatus(
                app_name=request.app_name,
                namespace=namespace,
                status="installing",
                message=f"Deploying {app_config.display_name}",
                timestamp=datetime.now()
            )

        except RuntimeError as e:
            raise RuntimeError(f"Deployment failed: {str(e)}")

    def remove_app(self, app_name: str, namespace: str = None) -> bool:
        """Remove an application from Kubernetes using Helm uninstall.

        Args:
            app_name: Name of the app to remove
            namespace: Namespace where app is deployed (uses app_name if not provided)

        Returns:
            True if removal was successful

        Raises:
            RuntimeError: If removal fails
        """
        # Use app name as namespace if not provided
        if namespace is None:
            namespace = app_name

        try:
            # First, try to uninstall with Helm
            try:
                self._run_helm_command([
                    "uninstall", app_name,
                    "-n", namespace
                ])
            except RuntimeError:
                # Helm release might not exist, continue to delete namespace
                pass

            # Also delete the namespace to clean up any remaining resources
            core_api = self._k8s.get_core_v1_api()
            try:
                core_api.delete_namespace(
                    name=namespace,
                    body=client.V1DeleteOptions(propagation_policy="Foreground")
                )
            except ApiException as e:
                if e.status != 404:
                    raise RuntimeError(f"Failed to delete namespace: {e.reason}")

            return True

        except ApiException as e:
            raise RuntimeError(f"Removal failed: {e.reason}")

    def update_app(
        self,
        app_name: str,
        namespace: str,
        new_config: Dict
    ) -> DeploymentStatus:
        """Update an application's configuration.

        Args:
            app_name: Name of the app to update
            namespace: Namespace where app is deployed
            new_config: New configuration to apply

        Returns:
            DeploymentStatus with result

        Raises:
            ValueError: If app not found
            RuntimeError: If update fails
        """
        # Get current app config
        app_config = self._catalog.get_app(app_name)
        if not app_config:
            raise ValueError(f"App '{app_name}' not found in catalog")

        # Apply new config
        app_config = self._apply_custom_config(app_config, new_config)

        try:
            # Update Deployment
            apps_api = self._k8s.get_apps_v1_api()
            deployment = self._build_deployment(app_config, namespace)

            apps_api.patch_namespaced_deployment(
                name=app_name,
                namespace=namespace,
                body=deployment
            )

            return DeploymentStatus(
                app_name=app_name,
                namespace=namespace,
                status="updated",
                message=f"Successfully updated {app_config.display_name}",
                timestamp=datetime.now()
            )

        except ApiException as e:
            raise RuntimeError(f"Update failed: {e.reason}")

    def get_deployed_apps(self, namespace: str = None) -> List[str]:
        """Get list of deployed app names.

        Args:
            namespace: Namespace to check (checks all namespaces if not provided)

        Returns:
            List of app names
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            # If no namespace specified, find all namespaces matching catalog apps
            if namespace is None:
                # Get all app names from catalog
                catalog_apps = {app.name for app in self._catalog.get_all_apps()}

                namespaces = core_api.list_namespace()
                app_names = []

                for ns in namespaces.items:
                    ns_name = ns.metadata.name
                    # Only check namespaces that match catalog app names
                    if ns_name in catalog_apps:
                        health = self.check_namespace_health(ns_name)
                        if health.get("deployments"):
                            app_names.append(ns_name)

                return app_names
            else:
                # Check specific namespace
                apps_api = self._k8s.get_apps_v1_api()
                deployments = apps_api.list_namespaced_deployment(
                    namespace=namespace
                )
                return [d.metadata.name for d in deployments.items]
        except ApiException:
            return []

    def _ensure_namespace(self, namespace: str, dry_run: bool = False) -> None:
        """Ensure namespace exists, create if not.

        Args:
            namespace: Namespace name
            dry_run: If True, don't actually create
        """
        if dry_run:
            return

        core_api = self._k8s.get_core_v1_api()
        try:
            core_api.read_namespace(name=namespace)
        except ApiException as e:
            if e.status == 404:
                # Namespace doesn't exist, create it
                ns = client.V1Namespace(
                    metadata=client.V1ObjectMeta(name=namespace)
                )
                core_api.create_namespace(body=ns)

    def _create_app_configmaps(
        self,
        app_name: str,
        namespace: str,
        dry_run: bool = False
    ) -> None:
        """Create app-specific ConfigMaps.

        Args:
            app_name: Name of the app
            namespace: Target namespace
            dry_run: If True, don't actually create
        """
        if dry_run:
            return

        # qBittorrent requires special configuration for auth bypass
        if app_name == "qbittorrent":
            self._create_qbittorrent_configmaps(namespace)

    def _create_qbittorrent_configmaps(self, namespace: str) -> None:
        """Create ConfigMaps for qBittorrent with auth bypass settings.

        Args:
            namespace: Target namespace
        """
        core_api = self._k8s.get_core_v1_api()

        # Init script ConfigMap - applies immutable config on startup
        init_script = """#!/bin/bash
# Apply immutable qBittorrent configuration
# This overwrites the config on every startup to ensure settings are immutable

CONFIG_DIR="/config/qBittorrent"
CONFIG_FILE="${CONFIG_DIR}/qBittorrent.conf"
IMMUTABLE_CONFIG="/config-immutable/qBittorrent.conf"

mkdir -p "$CONFIG_DIR"

echo "Applying immutable qBittorrent configuration..."
cp "$IMMUTABLE_CONFIG" "$CONFIG_FILE"

echo "Configuration applied:"
cat "$CONFIG_FILE"
"""
        init_configmap = client.V1ConfigMap(
            metadata=client.V1ObjectMeta(
                name="qbittorrent-init",
                namespace=namespace,
                labels={
                    "app": "qbittorrent",
                    "managed-by": "kubarr"
                }
            ),
            data={"99-apply-config.sh": init_script}
        )

        try:
            core_api.create_namespaced_config_map(namespace=namespace, body=init_configmap)
        except ApiException as e:
            if e.status != 409:  # Already exists
                raise

        # qBittorrent.conf ConfigMap with auth bypass settings
        qbittorrent_conf = """[AutoRun]
enabled=false
program=

[BitTorrent]
Session\\AddTorrentStopped=false
Session\\DefaultSavePath=/downloads/
Session\\Port=6881
Session\\QueueingSystemEnabled=true
Session\\ShareLimitAction=Stop
Session\\TempPath=/downloads/incomplete/

[LegalNotice]
Accepted=true

[Network]
PortForwardingEnabled=false
Proxy\\HostnameLookupEnabled=false
Proxy\\Profiles\\BitTorrent=true
Proxy\\Profiles\\Misc=true
Proxy\\Profiles\\RSS=true

[Preferences]
Connection\\PortRangeMin=6881
Connection\\UPnP=false
Downloads\\SavePath=/downloads/
Downloads\\TempPath=/downloads/incomplete/
WebUI\\Address=*
WebUI\\ServerDomains=*
WebUI\\LocalHostAuth=false
WebUI\\AuthSubnetWhitelistEnabled=true
WebUI\\AuthSubnetWhitelist=10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
WebUI\\CSRFProtection=false
WebUI\\ClickjackingProtection=false
WebUI\\HostHeaderValidation=false
"""
        conf_configmap = client.V1ConfigMap(
            metadata=client.V1ObjectMeta(
                name="qbittorrent-conf",
                namespace=namespace,
                labels={
                    "app": "qbittorrent",
                    "managed-by": "kubarr"
                }
            ),
            data={"qBittorrent.conf": qbittorrent_conf}
        )

        try:
            core_api.create_namespaced_config_map(namespace=namespace, body=conf_configmap)
        except ApiException as e:
            if e.status != 409:  # Already exists
                raise

    def _create_pvc(
        self,
        app_name: str,
        namespace: str,
        volume: "VolumeConfig",
        dry_run: bool = False
    ) -> None:
        """Create a PersistentVolumeClaim.

        Args:
            app_name: App name for labeling
            namespace: Target namespace
            volume: Volume configuration
            dry_run: If True, don't actually create
        """
        if dry_run:
            return

        core_api = self._k8s.get_core_v1_api()

        pvc = client.V1PersistentVolumeClaim(
            metadata=client.V1ObjectMeta(
                name=f"{app_name}-{volume.name}",
                namespace=namespace,
                labels={
                    "app": app_name,
                    "managed-by": "kubarr",
                    "volume": volume.name
                }
            ),
            spec=client.V1PersistentVolumeClaimSpec(
                access_modes=["ReadWriteOnce"],
                resources=client.V1ResourceRequirements(
                    requests={"storage": volume.size}
                ),
                storage_class_name=volume.storage_class
            )
        )

        try:
            core_api.create_namespaced_persistent_volume_claim(
                namespace=namespace,
                body=pvc
            )
        except ApiException as e:
            if e.status != 409:  # Already exists
                raise

    def _create_deployment(
        self,
        app_config: AppConfig,
        namespace: str,
        dry_run: bool = False
    ) -> None:
        """Create a Deployment.

        Args:
            app_config: App configuration
            namespace: Target namespace
            dry_run: If True, don't actually create
        """
        if dry_run:
            return

        apps_api = self._k8s.get_apps_v1_api()
        deployment = self._build_deployment(app_config, namespace)

        apps_api.create_namespaced_deployment(
            namespace=namespace,
            body=deployment
        )

    def _build_deployment(
        self,
        app_config: AppConfig,
        namespace: str
    ) -> client.V1Deployment:
        """Build a Deployment manifest.

        Args:
            app_config: App configuration
            namespace: Target namespace

        Returns:
            V1Deployment object
        """
        # Build volume mounts
        volume_mounts = [
            client.V1VolumeMount(
                name=vol.name,
                mount_path=vol.mount_path
            )
            for vol in app_config.volumes
        ]

        # Build volumes
        volumes = [
            client.V1Volume(
                name=vol.name,
                persistent_volume_claim=client.V1PersistentVolumeClaimVolumeSource(
                    claim_name=f"{app_config.name}-{vol.name}"
                )
            )
            for vol in app_config.volumes
        ]

        # Add qBittorrent-specific volumes for auth bypass
        if app_config.name == "qbittorrent":
            # Init scripts volume
            volume_mounts.append(
                client.V1VolumeMount(
                    name="init-scripts",
                    mount_path="/custom-cont-init.d"
                )
            )
            volumes.append(
                client.V1Volume(
                    name="init-scripts",
                    config_map=client.V1ConfigMapVolumeSource(
                        name="qbittorrent-init",
                        default_mode=0o755
                    )
                )
            )

            # Immutable config volume
            volume_mounts.append(
                client.V1VolumeMount(
                    name="qbittorrent-conf",
                    mount_path="/config-immutable",
                    read_only=True
                )
            )
            volumes.append(
                client.V1Volume(
                    name="qbittorrent-conf",
                    config_map=client.V1ConfigMapVolumeSource(
                        name="qbittorrent-conf"
                    )
                )
            )

        # Build environment variables
        env_vars = [
            client.V1EnvVar(name=k, value=v)
            for k, v in app_config.environment_variables.items()
        ]

        # Build container
        container = client.V1Container(
            name=app_config.name,
            image=app_config.container_image,
            ports=[client.V1ContainerPort(container_port=app_config.default_port)],
            env=env_vars,
            volume_mounts=volume_mounts,
            resources=client.V1ResourceRequirements(
                requests={
                    "cpu": app_config.resource_requirements.cpu_request,
                    "memory": app_config.resource_requirements.memory_request
                },
                limits={
                    "cpu": app_config.resource_requirements.cpu_limit,
                    "memory": app_config.resource_requirements.memory_limit
                }
            )
        )

        # Build deployment
        return client.V1Deployment(
            metadata=client.V1ObjectMeta(
                name=app_config.name,
                namespace=namespace,
                labels={
                    "app": app_config.name,
                    "managed-by": "kubarr",
                    "category": app_config.category
                }
            ),
            spec=client.V1DeploymentSpec(
                replicas=1,
                selector=client.V1LabelSelector(
                    match_labels={"app": app_config.name}
                ),
                template=client.V1PodTemplateSpec(
                    metadata=client.V1ObjectMeta(
                        labels={"app": app_config.name}
                    ),
                    spec=client.V1PodSpec(
                        containers=[container],
                        volumes=volumes
                    )
                )
            )
        )

    def _create_service(
        self,
        app_config: AppConfig,
        namespace: str,
        dry_run: bool = False
    ) -> None:
        """Create a Service.

        Args:
            app_config: App configuration
            namespace: Target namespace
            dry_run: If True, don't actually create
        """
        if dry_run:
            return

        core_api = self._k8s.get_core_v1_api()

        service = client.V1Service(
            metadata=client.V1ObjectMeta(
                name=app_config.name,
                namespace=namespace,
                labels={
                    "app": app_config.name,
                    "managed-by": "kubarr"
                }
            ),
            spec=client.V1ServiceSpec(
                selector={"app": app_config.name},
                ports=[
                    client.V1ServicePort(
                        name="http",
                        port=app_config.default_port,
                        target_port=app_config.default_port,
                        protocol="TCP"
                    )
                ],
                type="ClusterIP"
            )
        )

        core_api.create_namespaced_service(
            namespace=namespace,
            body=service
        )

    def _apply_custom_config(
        self,
        app_config: AppConfig,
        custom_config: Dict
    ) -> AppConfig:
        """Apply custom configuration overrides.

        Args:
            app_config: Base app configuration
            custom_config: Custom overrides

        Returns:
            Updated AppConfig
        """
        # Create a copy with custom overrides
        config_dict = app_config.model_dump()
        config_dict.update(custom_config)
        return AppConfig(**config_dict)

    def check_namespace_health(self, namespace: str) -> Dict:
        """Check if all deployments in a namespace are healthy.

        Args:
            namespace: Namespace to check

        Returns:
            Dict with status and details
        """
        try:
            apps_api = self._k8s.get_apps_v1_api()
            core_api = self._k8s.get_core_v1_api()

            # Check if namespace exists
            try:
                core_api.read_namespace(name=namespace)
            except ApiException as e:
                if e.status == 404:
                    return {
                        "status": "not_found",
                        "healthy": False,
                        "message": "Namespace does not exist"
                    }
                raise

            # Get all deployments in namespace (no label filter - namespace name identifies the app)
            deployments = apps_api.list_namespaced_deployment(
                namespace=namespace
            )

            if not deployments.items:
                return {
                    "status": "no_deployments",
                    "healthy": False,
                    "message": "No deployments found in namespace"
                }

            # Check each deployment's health
            all_healthy = True
            deployment_statuses = []

            for deployment in deployments.items:
                replicas = deployment.spec.replicas or 1
                ready_replicas = deployment.status.ready_replicas or 0
                available_replicas = deployment.status.available_replicas or 0

                is_healthy = (
                    ready_replicas == replicas and
                    available_replicas == replicas and
                    bool(deployment.status.conditions)
                )

                # Check conditions
                if deployment.status.conditions:
                    for condition in deployment.status.conditions:
                        if condition.type == "Available" and condition.status != "True":
                            is_healthy = False
                        elif condition.type == "Progressing" and condition.status != "True":
                            is_healthy = False

                deployment_statuses.append({
                    "name": deployment.metadata.name,
                    "replicas": replicas,
                    "ready_replicas": ready_replicas,
                    "available_replicas": available_replicas,
                    "healthy": is_healthy
                })

                if not is_healthy:
                    all_healthy = False

            return {
                "status": "healthy" if all_healthy else "unhealthy",
                "healthy": all_healthy,
                "deployments": deployment_statuses,
                "message": "All deployments healthy" if all_healthy else "Some deployments are not healthy"
            }

        except ApiException as e:
            return {
                "status": "error",
                "healthy": False,
                "message": f"Failed to check health: {e.reason}"
            }

    def check_namespace_exists(self, namespace: str) -> bool:
        """Check if a namespace exists.

        Args:
            namespace: Namespace name

        Returns:
            True if namespace exists, False otherwise
        """
        try:
            core_api = self._k8s.get_core_v1_api()
            core_api.read_namespace(name=namespace)
            return True
        except ApiException as e:
            if e.status == 404:
                return False
            raise
