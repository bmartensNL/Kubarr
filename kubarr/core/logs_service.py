"""Logs service for Kubarr applications."""

from datetime import datetime
from typing import Generator, List, Optional

from kubernetes.client.rest import ApiException

from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import LogEntry, LogFilter


class LogsService:
    """Provides log retrieval and streaming capabilities."""

    def __init__(self, k8s_client: K8sClientManager) -> None:
        """Initialize the logs service.

        Args:
            k8s_client: Kubernetes client manager
        """
        self._k8s = k8s_client

    def get_logs(self, log_filter: LogFilter) -> List[LogEntry]:
        """Get logs based on filter criteria.

        Args:
            log_filter: Filter criteria for logs

        Returns:
            List of LogEntry objects
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            # If pod name is specified, get logs from that pod
            if log_filter.pod_name:
                return self._get_pod_logs(log_filter)

            # If app name is specified, get logs from all pods with that label
            if log_filter.app_name:
                label_selector = f"app={log_filter.app_name}"
                pods = core_api.list_namespaced_pod(
                    namespace=log_filter.namespace,
                    label_selector=label_selector
                )

                all_logs = []
                for pod in pods.items:
                    pod_filter = LogFilter(
                        namespace=log_filter.namespace,
                        pod_name=pod.metadata.name,
                        container=log_filter.container,
                        since=log_filter.since,
                        tail_lines=log_filter.tail_lines,
                        follow=False  # Don't follow for multiple pods
                    )
                    all_logs.extend(self._get_pod_logs(pod_filter))

                # Sort by timestamp
                all_logs.sort(key=lambda x: x.timestamp)

                # Return only tail_lines if specified
                if log_filter.tail_lines:
                    return all_logs[-log_filter.tail_lines:]

                return all_logs

            return []

        except ApiException:
            return []

    def stream_logs(self, log_filter: LogFilter) -> Generator[LogEntry, None, None]:
        """Stream logs in real-time.

        Args:
            log_filter: Filter criteria for logs

        Yields:
            LogEntry objects as they arrive
        """
        if not log_filter.pod_name:
            raise ValueError("pod_name is required for streaming logs")

        try:
            core_api = self._k8s.get_core_v1_api()

            # Determine container name
            container = log_filter.container
            if not container:
                # Get first container
                pod = core_api.read_namespaced_pod(
                    name=log_filter.pod_name,
                    namespace=log_filter.namespace
                )
                if pod.spec.containers:
                    container = pod.spec.containers[0].name

            # Stream logs
            log_stream = core_api.read_namespaced_pod_log(
                name=log_filter.pod_name,
                namespace=log_filter.namespace,
                container=container,
                follow=True,
                tail_lines=log_filter.tail_lines,
                _preload_content=False
            )

            for line in log_stream:
                if isinstance(line, bytes):
                    line = line.decode("utf-8")

                line = line.strip()
                if not line:
                    continue

                yield LogEntry(
                    timestamp=datetime.now(),
                    pod_name=log_filter.pod_name,
                    container=container,
                    message=line,
                    level=self._detect_log_level(line)
                )

        except ApiException:
            return

    def get_pod_logs(
        self,
        pod_name: str,
        namespace: str,
        container: Optional[str] = None,
        tail: int = 100
    ) -> str:
        """Get raw logs from a specific pod.

        Args:
            pod_name: Pod name
            namespace: Namespace
            container: Container name (optional, uses first if not specified)
            tail: Number of lines to return

        Returns:
            Raw log string
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            # Determine container name
            if not container:
                # Get first container
                pod = core_api.read_namespaced_pod(
                    name=pod_name,
                    namespace=namespace
                )
                if pod.spec.containers:
                    container = pod.spec.containers[0].name

            logs = core_api.read_namespaced_pod_log(
                name=pod_name,
                namespace=namespace,
                container=container,
                tail_lines=tail
            )

            return logs

        except ApiException:
            return ""

    def _get_pod_logs(self, log_filter: LogFilter) -> List[LogEntry]:
        """Get logs from a specific pod.

        Args:
            log_filter: Filter with pod_name specified

        Returns:
            List of LogEntry objects
        """
        try:
            core_api = self._k8s.get_core_v1_api()

            # Determine container name
            container = log_filter.container
            if not container:
                # Get first container
                pod = core_api.read_namespaced_pod(
                    name=log_filter.pod_name,
                    namespace=log_filter.namespace
                )
                if pod.spec.containers:
                    container = pod.spec.containers[0].name

            # Build kwargs for log request
            log_kwargs = {
                "name": log_filter.pod_name,
                "namespace": log_filter.namespace,
                "container": container,
            }

            if log_filter.tail_lines:
                log_kwargs["tail_lines"] = log_filter.tail_lines

            if log_filter.since:
                # Calculate seconds since
                since_seconds = int((datetime.now() - log_filter.since).total_seconds())
                log_kwargs["since_seconds"] = since_seconds

            # Get logs
            logs = core_api.read_namespaced_pod_log(**log_kwargs)

            # Parse logs into entries
            entries = []
            for line in logs.split("\n"):
                line = line.strip()
                if not line:
                    continue

                entries.append(LogEntry(
                    timestamp=datetime.now(),  # TODO: Parse actual timestamp from log line
                    pod_name=log_filter.pod_name,
                    container=container,
                    message=line,
                    level=self._detect_log_level(line)
                ))

            return entries

        except ApiException:
            return []

    @staticmethod
    def _detect_log_level(line: str) -> Optional[str]:
        """Attempt to detect log level from log line.

        Args:
            line: Log line

        Returns:
            Log level (ERROR, WARN, INFO, DEBUG) or None
        """
        line_upper = line.upper()

        if "ERROR" in line_upper or "FATAL" in line_upper:
            return "ERROR"
        elif "WARN" in line_upper or "WARNING" in line_upper:
            return "WARN"
        elif "INFO" in line_upper:
            return "INFO"
        elif "DEBUG" in line_upper or "TRACE" in line_upper:
            return "DEBUG"

        return None
