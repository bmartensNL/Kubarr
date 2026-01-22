"""Deployment manager for Kubarr applications."""

from datetime import datetime
from typing import Dict, List, Optional

from kubernetes import client
from kubernetes.client.rest import ApiException

from kubarr.core.app_catalog import AppCatalog
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppConfig, DeploymentRequest, DeploymentStatus


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

    def deploy_app(
        self,
        request: DeploymentRequest,
        dry_run: bool = False
    ) -> DeploymentStatus:
        """Deploy an application to Kubernetes.

        Args:
            request: Deployment request with app name and config
            dry_run: If True, validate but don't actually deploy

        Returns:
            DeploymentStatus with result

        Raises:
            ValueError: If app not found in catalog
            RuntimeError: If deployment fails
        """
        # Get app config from catalog
        app_config = self._catalog.get_app(request.app_name)
        if not app_config:
            raise ValueError(f"App '{request.app_name}' not found in catalog")

        # Apply custom config overrides if provided
        if request.custom_config:
            app_config = self._apply_custom_config(app_config, request.custom_config)

        try:
            # Create namespace if it doesn't exist
            self._ensure_namespace(request.namespace, dry_run)

            # Create PersistentVolumeClaims
            for volume in app_config.volumes:
                self._create_pvc(
                    app_name=request.app_name,
                    namespace=request.namespace,
                    volume=volume,
                    dry_run=dry_run
                )

            # Create Deployment
            self._create_deployment(
                app_config=app_config,
                namespace=request.namespace,
                dry_run=dry_run
            )

            # Create Service
            self._create_service(
                app_config=app_config,
                namespace=request.namespace,
                dry_run=dry_run
            )

            return DeploymentStatus(
                app_name=request.app_name,
                namespace=request.namespace,
                status="deployed" if not dry_run else "dry-run",
                message=f"Successfully deployed {app_config.display_name}",
                timestamp=datetime.now()
            )

        except ApiException as e:
            raise RuntimeError(f"Deployment failed: {e.reason}")

    def remove_app(self, app_name: str, namespace: str) -> bool:
        """Remove an application from Kubernetes.

        Args:
            app_name: Name of the app to remove
            namespace: Namespace where app is deployed

        Returns:
            True if removal was successful

        Raises:
            RuntimeError: If removal fails
        """
        try:
            apps_api = self._k8s.get_apps_v1_api()
            core_api = self._k8s.get_core_v1_api()

            # Delete Deployment
            try:
                apps_api.delete_namespaced_deployment(
                    name=app_name,
                    namespace=namespace,
                    body=client.V1DeleteOptions(propagation_policy="Foreground")
                )
            except ApiException as e:
                if e.status != 404:
                    raise

            # Delete Service
            try:
                core_api.delete_namespaced_service(
                    name=app_name,
                    namespace=namespace
                )
            except ApiException as e:
                if e.status != 404:
                    raise

            # Delete PVCs
            try:
                pvcs = core_api.list_namespaced_persistent_volume_claim(
                    namespace=namespace,
                    label_selector=f"app={app_name}"
                )
                for pvc in pvcs.items:
                    core_api.delete_namespaced_persistent_volume_claim(
                        name=pvc.metadata.name,
                        namespace=namespace
                    )
            except ApiException as e:
                if e.status != 404:
                    raise

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

    def get_deployed_apps(self, namespace: str) -> List[str]:
        """Get list of deployed app names in a namespace.

        Args:
            namespace: Namespace to check

        Returns:
            List of app names
        """
        try:
            apps_api = self._k8s.get_apps_v1_api()
            deployments = apps_api.list_namespaced_deployment(
                namespace=namespace,
                label_selector="managed-by=kubarr"
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
