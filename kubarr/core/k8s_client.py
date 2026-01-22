"""Kubernetes client manager for Kubarr."""

import os
from typing import Optional

from kubernetes import client, config
from kubernetes.client import CoreV1Api, AppsV1Api, CustomObjectsApi
from kubernetes.client.rest import ApiException


class K8sClientManager:
    """Manages Kubernetes client connections.

    This class provides a centralized way to manage Kubernetes API clients
    with support for both local (kubeconfig) and in-cluster authentication.
    """

    def __init__(
        self,
        kubeconfig_path: Optional[str] = None,
        in_cluster: bool = False
    ) -> None:
        """Initialize the Kubernetes client manager.

        Args:
            kubeconfig_path: Path to kubeconfig file (for CLI/local usage)
            in_cluster: Use in-cluster config (for dashboard pod)
        """
        self._kubeconfig_path = kubeconfig_path
        self._in_cluster = in_cluster or os.getenv("KUBARR_IN_CLUSTER", "").lower() == "true"
        self._load_config()

    def _load_config(self) -> None:
        """Load Kubernetes configuration."""
        if self._in_cluster:
            # Running inside the cluster, use service account
            try:
                config.load_incluster_config()
            except config.ConfigException as e:
                raise RuntimeError(f"Failed to load in-cluster config: {e}")
        elif self._kubeconfig_path:
            # Use specified kubeconfig file
            try:
                config.load_kube_config(config_file=self._kubeconfig_path)
            except config.ConfigException as e:
                raise RuntimeError(f"Failed to load kubeconfig from {self._kubeconfig_path}: {e}")
        else:
            # Try default kubeconfig location
            try:
                config.load_kube_config()
            except config.ConfigException as e:
                raise RuntimeError(f"Failed to load default kubeconfig: {e}")

    def get_core_v1_api(self) -> CoreV1Api:
        """Get CoreV1Api client for basic Kubernetes operations.

        Returns:
            CoreV1Api client instance
        """
        return client.CoreV1Api()

    def get_apps_v1_api(self) -> AppsV1Api:
        """Get AppsV1Api client for managing Deployments and StatefulSets.

        Returns:
            AppsV1Api client instance
        """
        return client.AppsV1Api()

    def get_custom_objects_api(self) -> CustomObjectsApi:
        """Get CustomObjectsApi client for metrics and custom resources.

        Returns:
            CustomObjectsApi client instance
        """
        return client.CustomObjectsApi()

    def test_connection(self) -> bool:
        """Test the Kubernetes connection.

        Returns:
            True if connection is successful

        Raises:
            RuntimeError: If connection test fails
        """
        try:
            core_api = self.get_core_v1_api()
            # Try to list namespaces as a connection test
            core_api.list_namespace(limit=1)
            return True
        except ApiException as e:
            raise RuntimeError(f"Kubernetes connection test failed: {e}")

    def get_server_version(self) -> str:
        """Get Kubernetes server version.

        Returns:
            Kubernetes version string
        """
        try:
            version_api = client.VersionApi()
            version = version_api.get_code()
            return f"{version.major}.{version.minor}"
        except ApiException:
            return "unknown"

    def check_metrics_server_available(self) -> bool:
        """Check if metrics-server is available in the cluster.

        Returns:
            True if metrics-server is available
        """
        try:
            custom_api = self.get_custom_objects_api()
            # Try to list pod metrics
            custom_api.list_cluster_custom_object(
                group="metrics.k8s.io",
                version="v1beta1",
                plural="pods",
                limit=1
            )
            return True
        except ApiException:
            return False
