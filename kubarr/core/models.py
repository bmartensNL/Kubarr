"""Pydantic models for Kubarr."""

from datetime import datetime, timedelta
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


# Resource Configuration Models

class ResourceRequirements(BaseModel):
    """Resource requirements for an application."""

    cpu_request: str = Field(default="100m", description="CPU request")
    cpu_limit: str = Field(default="1000m", description="CPU limit")
    memory_request: str = Field(default="128Mi", description="Memory request")
    memory_limit: str = Field(default="512Mi", description="Memory limit")


class VolumeConfig(BaseModel):
    """Volume configuration for an application."""

    name: str = Field(..., description="Volume name")
    mount_path: str = Field(..., description="Mount path in container")
    size: str = Field(default="10Gi", description="Volume size")
    storage_class: Optional[str] = Field(default=None, description="Storage class name")


# App Catalog Models

class AppConfig(BaseModel):
    """Configuration for an application in the catalog."""

    name: str = Field(..., description="App identifier (lowercase, no spaces)")
    display_name: str = Field(..., description="Human-readable name")
    description: str = Field(..., description="App description")
    icon: Optional[str] = Field(default=None, description="Icon URL or emoji")
    version: str = Field(default="latest", description="Default version/tag")
    container_image: str = Field(..., description="Container image")
    default_port: int = Field(..., description="Default service port")
    resource_requirements: ResourceRequirements = Field(
        default_factory=ResourceRequirements,
        description="Resource requirements"
    )
    environment_variables: Dict[str, str] = Field(
        default_factory=dict,
        description="Default environment variables"
    )
    volumes: List[VolumeConfig] = Field(
        default_factory=list,
        description="Volume mounts"
    )
    category: str = Field(default="media", description="App category")
    is_system: bool = Field(default=False, description="System app that cannot be uninstalled")
    is_hidden: bool = Field(default=False, description="Hidden app without Open button")


# Deployment Models

class DeploymentRequest(BaseModel):
    """Request to deploy an application."""

    app_name: str = Field(..., description="App to deploy")
    namespace: str = Field(default="media", description="Target namespace")
    custom_config: Optional[Dict[str, Any]] = Field(
        default=None,
        description="Custom configuration overrides"
    )


class DeploymentStatus(BaseModel):
    """Status of a deployment operation."""

    app_name: str = Field(..., description="App name")
    namespace: str = Field(..., description="Namespace")
    status: str = Field(..., description="Status: pending, deploying, running, failed")
    message: Optional[str] = Field(default=None, description="Status message")
    timestamp: datetime = Field(default_factory=datetime.now, description="Status timestamp")


# Monitoring Models

class PodStatus(BaseModel):
    """Status of a pod."""

    name: str = Field(..., description="Pod name")
    app: str = Field(..., description="App label")
    namespace: str = Field(..., description="Namespace")
    status: str = Field(..., description="Pod phase: Running, Pending, Failed, etc.")
    ready: bool = Field(..., description="Whether pod is ready")
    restart_count: int = Field(default=0, description="Container restart count")
    age: str = Field(..., description="Pod age")
    node: Optional[str] = Field(default=None, description="Node name")
    ip: Optional[str] = Field(default=None, description="Pod IP")


class PodMetrics(BaseModel):
    """Resource metrics for a pod."""

    name: str = Field(..., description="Pod name")
    namespace: str = Field(..., description="Namespace")
    cpu_usage: str = Field(..., description="CPU usage (e.g., '100m')")
    memory_usage: str = Field(..., description="Memory usage (e.g., '256Mi')")
    timestamp: datetime = Field(default_factory=datetime.now, description="Metrics timestamp")


class ServiceEndpoint(BaseModel):
    """Service endpoint information."""

    name: str = Field(..., description="Service name")
    namespace: str = Field(..., description="Namespace")
    port: int = Field(..., description="Service port")
    target_port: int = Field(..., description="Target pod port")
    port_forward_command: str = Field(..., description="kubectl port-forward command")
    url: Optional[str] = Field(default=None, description="External URL if using Ingress")
    type: str = Field(default="ClusterIP", description="Service type")


class AppHealth(BaseModel):
    """Overall health status of an application."""

    app_name: str = Field(..., description="App name")
    namespace: str = Field(..., description="Namespace")
    healthy: bool = Field(..., description="Whether app is healthy")
    pods: List[PodStatus] = Field(default_factory=list, description="Pod statuses")
    metrics: Optional[List[PodMetrics]] = Field(default=None, description="Resource metrics")
    endpoints: List[ServiceEndpoint] = Field(default_factory=list, description="Service endpoints")
    message: Optional[str] = Field(default=None, description="Health message")


# Log Models

class LogEntry(BaseModel):
    """A single log entry."""

    timestamp: datetime = Field(..., description="Log timestamp")
    pod_name: str = Field(..., description="Pod name")
    container: str = Field(..., description="Container name")
    message: str = Field(..., description="Log message")
    level: Optional[str] = Field(default=None, description="Log level if parsed")


class LogFilter(BaseModel):
    """Filter for log queries."""

    namespace: str = Field(default="media", description="Namespace to query")
    app_name: Optional[str] = Field(default=None, description="Filter by app name")
    pod_name: Optional[str] = Field(default=None, description="Filter by pod name")
    container: Optional[str] = Field(default=None, description="Filter by container name")
    since: Optional[datetime] = Field(default=None, description="Start time")
    tail_lines: int = Field(default=100, description="Number of lines to return")
    follow: bool = Field(default=False, description="Follow log stream")


# System Info Models

class SystemInfo(BaseModel):
    """System information."""

    kubarr_version: str = Field(..., description="Kubarr version")
    kubernetes_version: str = Field(..., description="Kubernetes version")
    in_cluster: bool = Field(..., description="Running in cluster")
    metrics_server_available: bool = Field(..., description="Metrics server available")


class ClusterInfo(BaseModel):
    """Kubernetes cluster information."""

    name: str = Field(..., description="Cluster name")
    server: str = Field(..., description="API server URL")
    kubernetes_version: str = Field(..., description="Kubernetes version")
    node_count: int = Field(..., description="Number of nodes")


# App Instance Models

class AppInfo(BaseModel):
    """Information about an installed app."""

    name: str = Field(..., description="App name")
    display_name: str = Field(..., description="Display name")
    namespace: str = Field(..., description="Namespace")
    status: str = Field(..., description="Overall status")
    pod_count: int = Field(..., description="Number of pods")
    ready_pods: int = Field(..., description="Number of ready pods")
    installed_at: Optional[datetime] = Field(default=None, description="Installation time")


class AppDetail(BaseModel):
    """Detailed information about an installed app."""

    name: str = Field(..., description="App name")
    display_name: str = Field(..., description="Display name")
    namespace: str = Field(..., description="Namespace")
    config: AppConfig = Field(..., description="App configuration")
    health: AppHealth = Field(..., description="Health status")
    installed_at: Optional[datetime] = Field(default=None, description="Installation time")
