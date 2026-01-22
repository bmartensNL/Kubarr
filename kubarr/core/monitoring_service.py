"""Monitoring service for Kubarr applications."""

from datetime import datetime, timedelta
from typing import List, Optional

from kubernetes.client.rest import ApiException

from kubarr.core.app_catalog import AppCatalog
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppHealth, PodMetrics, PodStatus, ServiceEndpoint


class MonitoringService:
    """Provides monitoring and health check capabilities."""

    def __init__(
        self,
        k8s_client: K8sClientManager,
        catalog: Optional[AppCatalog] = None
    ) -> None:
        """Initialize the monitoring service.

        Args:
            k8s_client: Kubernetes client manager
            catalog: App catalog (creates new one if not provided)
        """
        self._k8s = k8s_client
        self._catalog = catalog or AppCatalog()
        self._metrics_available = self._k8s.check_metrics_server_available()

    def get_pod_status(
        self,
        namespace: str,
        app_name: Optional[str] = None
    ) -> List[PodStatus]:
        """Get status of pods in a namespace.

        Args:
            namespace: Namespace to query
            app_name: Optional filter by app name

        Returns:
            List of PodStatus objects
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            # Build label selector
            label_selector = None
            if app_name:
                label_selector = f"app={app_name}"

            pods = core_api.list_namespaced_pod(
                namespace=namespace,
                label_selector=label_selector
            )

            statuses = []
            for pod in pods.items:
                # Calculate pod age
                age = datetime.now(pod.metadata.creation_timestamp.tzinfo) - pod.metadata.creation_timestamp
                age_str = self._format_age(age)

                # Get restart count
                restart_count = 0
                if pod.status.container_statuses:
                    restart_count = sum(
                        cs.restart_count for cs in pod.status.container_statuses
                    )

                # Determine if pod is ready
                ready = False
                if pod.status.conditions:
                    for condition in pod.status.conditions:
                        if condition.type == "Ready":
                            ready = condition.status == "True"
                            break

                statuses.append(PodStatus(
                    name=pod.metadata.name,
                    app=pod.metadata.labels.get("app", "unknown"),
                    namespace=namespace,
                    status=pod.status.phase,
                    ready=ready,
                    restart_count=restart_count,
                    age=age_str,
                    node=pod.spec.node_name,
                    ip=pod.status.pod_ip
                ))

            return statuses

        except ApiException:
            return []

    def get_pod_metrics(
        self,
        namespace: str,
        app_name: Optional[str] = None
    ) -> List[PodMetrics]:
        """Get resource metrics for pods.

        Requires metrics-server to be installed in the cluster.

        Args:
            namespace: Namespace to query
            app_name: Optional filter by app name

        Returns:
            List of PodMetrics objects (empty if metrics-server not available)
        """
        if not self._metrics_available:
            return []

        try:
            custom_api = self._k8s.get_custom_objects_api()

            # Get pod metrics
            metrics = custom_api.list_namespaced_custom_object(
                group="metrics.k8s.io",
                version="v1beta1",
                namespace=namespace,
                plural="pods"
            )

            result = []
            for item in metrics.get("items", []):
                pod_name = item["metadata"]["name"]

                # Filter by app if specified
                if app_name:
                    labels = item["metadata"].get("labels", {})
                    if labels.get("app") != app_name:
                        continue

                # Calculate total resource usage across containers
                total_cpu = 0
                total_memory = 0

                for container in item["containers"]:
                    # Parse CPU (in nanocores)
                    cpu_str = container["usage"]["cpu"]
                    cpu_nanocores = self._parse_cpu(cpu_str)
                    total_cpu += cpu_nanocores

                    # Parse memory (in bytes)
                    memory_str = container["usage"]["memory"]
                    memory_bytes = self._parse_memory(memory_str)
                    total_memory += memory_bytes

                result.append(PodMetrics(
                    name=pod_name,
                    namespace=namespace,
                    cpu_usage=self._format_cpu(total_cpu),
                    memory_usage=self._format_memory(total_memory),
                    timestamp=datetime.now()
                ))

            return result

        except ApiException:
            return []

    def get_app_health(self, app_name: str, namespace: str) -> AppHealth:
        """Get overall health status for an application.

        Args:
            app_name: App name
            namespace: Namespace

        Returns:
            AppHealth object
        """
        # Get pod status
        pods = self.get_pod_status(namespace, app_name)

        # Get metrics if available
        metrics = None
        if self._metrics_available:
            metrics = self.get_pod_metrics(namespace, app_name)

        # Get service endpoints
        endpoints = self.get_service_endpoints(app_name, namespace)

        # Determine overall health
        healthy = True
        message = "All pods running"

        if not pods:
            healthy = False
            message = "No pods found"
        else:
            running_pods = [p for p in pods if p.status == "Running" and p.ready]
            if len(running_pods) != len(pods):
                healthy = False
                message = f"{len(running_pods)}/{len(pods)} pods ready"

            # Check for high restart counts
            high_restarts = [p for p in pods if p.restart_count > 5]
            if high_restarts:
                healthy = False
                message = "Pods restarting frequently"

        return AppHealth(
            app_name=app_name,
            namespace=namespace,
            healthy=healthy,
            pods=pods,
            metrics=metrics,
            endpoints=endpoints,
            message=message
        )

    def get_service_endpoints(
        self,
        app_name: str,
        namespace: str
    ) -> List[ServiceEndpoint]:
        """Get service endpoints for an app.

        Args:
            app_name: App name
            namespace: Namespace

        Returns:
            List of ServiceEndpoint objects
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            service = core_api.read_namespaced_service(
                name=app_name,
                namespace=namespace
            )

            endpoints = []
            for port in service.spec.ports:
                port_forward_cmd = (
                    f"kubectl port-forward -n {namespace} "
                    f"svc/{app_name} {port.port}:{port.port}"
                )

                # Check if there's an external URL (LoadBalancer or Ingress)
                external_url = None
                if service.spec.type == "LoadBalancer":
                    if service.status.load_balancer.ingress:
                        ip = service.status.load_balancer.ingress[0].ip
                        external_url = f"http://{ip}:{port.port}"

                endpoints.append(ServiceEndpoint(
                    name=app_name,
                    namespace=namespace,
                    port=port.port,
                    target_port=port.target_port,
                    port_forward_command=port_forward_cmd,
                    url=external_url,
                    type=service.spec.type
                ))

            return endpoints

        except ApiException:
            return []

    def check_metrics_server_available(self) -> bool:
        """Check if metrics-server is available.

        Returns:
            True if metrics-server is available
        """
        return self._metrics_available

    @staticmethod
    def _format_age(age: timedelta) -> str:
        """Format age timedelta as human-readable string.

        Args:
            age: Timedelta to format

        Returns:
            Formatted string like "5m", "2h", "3d"
        """
        total_seconds = int(age.total_seconds())

        if total_seconds < 60:
            return f"{total_seconds}s"
        elif total_seconds < 3600:
            return f"{total_seconds // 60}m"
        elif total_seconds < 86400:
            return f"{total_seconds // 3600}h"
        else:
            return f"{total_seconds // 86400}d"

    @staticmethod
    def _parse_cpu(cpu_str: str) -> int:
        """Parse CPU string to nanocores.

        Args:
            cpu_str: CPU string like "100m" or "1"

        Returns:
            CPU in nanocores
        """
        if cpu_str.endswith("n"):
            return int(cpu_str[:-1])
        elif cpu_str.endswith("u"):
            return int(cpu_str[:-1]) * 1000
        elif cpu_str.endswith("m"):
            return int(cpu_str[:-1]) * 1000000
        else:
            return int(cpu_str) * 1000000000

    @staticmethod
    def _format_cpu(nanocores: int) -> str:
        """Format CPU nanocores to readable string.

        Args:
            nanocores: CPU in nanocores

        Returns:
            Formatted string like "100m" or "1.5"
        """
        millicores = nanocores / 1000000
        if millicores < 1000:
            return f"{int(millicores)}m"
        else:
            cores = millicores / 1000
            return f"{cores:.2f}"

    @staticmethod
    def _parse_memory(memory_str: str) -> int:
        """Parse memory string to bytes.

        Args:
            memory_str: Memory string like "128Mi" or "1Gi"

        Returns:
            Memory in bytes
        """
        if memory_str.endswith("Ki"):
            return int(memory_str[:-2]) * 1024
        elif memory_str.endswith("Mi"):
            return int(memory_str[:-2]) * 1024 * 1024
        elif memory_str.endswith("Gi"):
            return int(memory_str[:-2]) * 1024 * 1024 * 1024
        elif memory_str.endswith("Ti"):
            return int(memory_str[:-2]) * 1024 * 1024 * 1024 * 1024
        else:
            return int(memory_str)

    @staticmethod
    def _format_memory(memory_bytes: int) -> str:
        """Format memory bytes to readable string.

        Args:
            memory_bytes: Memory in bytes

        Returns:
            Formatted string like "128Mi" or "1.5Gi"
        """
        if memory_bytes < 1024 * 1024:
            return f"{memory_bytes // 1024}Ki"
        elif memory_bytes < 1024 * 1024 * 1024:
            return f"{memory_bytes // (1024 * 1024)}Mi"
        else:
            gib = memory_bytes / (1024 * 1024 * 1024)
            return f"{gib:.2f}Gi"
