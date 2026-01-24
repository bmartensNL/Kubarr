"""Monitoring API router."""

import httpx
from typing import Any, Dict, List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel

from kubarr.api.dependencies import (
    get_app_catalog,
    get_current_active_user,
    get_k8s_client,
    get_monitoring_service,
)
from kubarr.core.app_catalog import AppCatalog
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppHealth, PodMetrics, PodStatus, ServiceEndpoint
from kubarr.core.models_auth import User
from kubarr.core.monitoring_service import MonitoringService

# Prometheus URL (inside cluster)
PROMETHEUS_URL = "http://prometheus.prometheus.svc.cluster.local:9090"

# Authentication via oauth2-proxy headers (X-Auth-Request-User)
router = APIRouter(dependencies=[Depends(get_current_active_user)])


class AppMetrics(BaseModel):
    """Metrics for a single app."""
    app_name: str
    namespace: str
    cpu_usage_cores: float  # Current CPU usage in cores
    memory_usage_bytes: int  # Current memory usage in bytes
    memory_usage_mb: float  # Current memory usage in MB
    cpu_usage_percent: Optional[float] = None  # CPU usage as percentage of request
    memory_usage_percent: Optional[float] = None  # Memory usage as percentage of limit


class ClusterMetrics(BaseModel):
    """Overall cluster metrics."""
    total_cpu_cores: float
    total_memory_bytes: int
    used_cpu_cores: float
    used_memory_bytes: int
    cpu_usage_percent: float
    memory_usage_percent: float
    container_count: int
    pod_count: int


class TimeSeriesPoint(BaseModel):
    """A single point in a time series."""
    timestamp: float
    value: float


class AppHistoricalMetrics(BaseModel):
    """Historical metrics for a single app."""
    app_name: str
    namespace: str
    cpu_series: List[TimeSeriesPoint]
    memory_series: List[TimeSeriesPoint]
    # Current values
    cpu_usage_cores: float
    memory_usage_bytes: int
    memory_usage_mb: float


class AppDetailMetrics(BaseModel):
    """Detailed metrics for a single app including pods."""
    app_name: str
    namespace: str
    historical: AppHistoricalMetrics
    pods: List[PodStatus]


async def query_prometheus(query: str) -> List[Dict[str, Any]]:
    """Query Prometheus and return results.

    Args:
        query: PromQL query string

    Returns:
        List of result items from Prometheus
    """
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.get(
                f"{PROMETHEUS_URL}/prometheus/api/v1/query",
                params={"query": query}
            )
            response.raise_for_status()
            data = response.json()

            if data.get("status") != "success":
                return []

            return data.get("data", {}).get("result", [])
    except Exception:
        return []


async def query_prometheus_range(
    query: str,
    start: float,
    end: float,
    step: str = "60s"
) -> List[Dict[str, Any]]:
    """Query Prometheus for a range of time and return results.

    Args:
        query: PromQL query string
        start: Start timestamp (Unix seconds)
        end: End timestamp (Unix seconds)
        step: Query resolution step (e.g., "60s", "5m")

    Returns:
        List of result items from Prometheus with time series data
    """
    try:
        async with httpx.AsyncClient(timeout=15.0) as client:
            response = await client.get(
                f"{PROMETHEUS_URL}/prometheus/api/v1/query_range",
                params={
                    "query": query,
                    "start": start,
                    "end": end,
                    "step": step
                }
            )
            response.raise_for_status()
            data = response.json()

            if data.get("status") != "success":
                return []

            return data.get("data", {}).get("result", [])
    except Exception:
        return []


@router.get("/prometheus/apps", response_model=List[AppMetrics])
async def get_app_metrics_from_prometheus(
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[AppMetrics]:
    """Get resource metrics for all installed apps from Prometheus.

    Returns:
        List of AppMetrics for each app with metrics available
    """
    # Get list of all known app names from catalog to filter metrics
    all_apps = catalog.get_all_apps()
    app_namespaces = {app.name for app in all_apps}
    # Also include monitoring namespaces
    app_namespaces.update({"prometheus", "loki", "promtail", "grafana", "kubarr-system"})

    # Query CPU usage by namespace (summed across all containers)
    cpu_query = 'sum by (namespace) (rate(container_cpu_usage_seconds_total{container!="",container!="POD"}[5m]))'
    cpu_results = await query_prometheus(cpu_query)

    # Query memory usage by namespace
    memory_query = 'sum by (namespace) (container_memory_working_set_bytes{container!="",container!="POD"})'
    memory_results = await query_prometheus(memory_query)

    # Build metrics dict by namespace
    metrics_by_namespace: Dict[str, AppMetrics] = {}

    # Process CPU results
    for result in cpu_results:
        namespace = result.get("metric", {}).get("namespace", "")
        if namespace in app_namespaces:
            value = float(result.get("value", [0, 0])[1])
            metrics_by_namespace[namespace] = AppMetrics(
                app_name=namespace,
                namespace=namespace,
                cpu_usage_cores=round(value, 4),
                memory_usage_bytes=0,
                memory_usage_mb=0
            )

    # Process memory results
    for result in memory_results:
        namespace = result.get("metric", {}).get("namespace", "")
        if namespace in app_namespaces:
            value = int(float(result.get("value", [0, 0])[1]))
            if namespace in metrics_by_namespace:
                metrics_by_namespace[namespace].memory_usage_bytes = value
                metrics_by_namespace[namespace].memory_usage_mb = round(value / (1024 * 1024), 2)
            else:
                metrics_by_namespace[namespace] = AppMetrics(
                    app_name=namespace,
                    namespace=namespace,
                    cpu_usage_cores=0,
                    memory_usage_bytes=value,
                    memory_usage_mb=round(value / (1024 * 1024), 2)
                )

    return list(metrics_by_namespace.values())


@router.get("/prometheus/cluster", response_model=ClusterMetrics)
async def get_cluster_metrics() -> ClusterMetrics:
    """Get overall cluster resource metrics from Prometheus.

    Returns:
        ClusterMetrics with CPU and memory usage
    """
    # Total CPU cores
    total_cpu_query = 'sum(machine_cpu_cores)'
    total_cpu_results = await query_prometheus(total_cpu_query)
    total_cpu = float(total_cpu_results[0]["value"][1]) if total_cpu_results else 0

    # Total memory
    total_memory_query = 'sum(machine_memory_bytes)'
    total_memory_results = await query_prometheus(total_memory_query)
    total_memory = int(float(total_memory_results[0]["value"][1])) if total_memory_results else 0

    # Used CPU
    used_cpu_query = 'sum(rate(container_cpu_usage_seconds_total{container!="",container!="POD"}[5m]))'
    used_cpu_results = await query_prometheus(used_cpu_query)
    used_cpu = float(used_cpu_results[0]["value"][1]) if used_cpu_results else 0

    # Used memory
    used_memory_query = 'sum(container_memory_working_set_bytes{container!="",container!="POD"})'
    used_memory_results = await query_prometheus(used_memory_query)
    used_memory = int(float(used_memory_results[0]["value"][1])) if used_memory_results else 0

    # Container count
    container_query = 'count(container_last_seen{container!="",container!="POD"})'
    container_results = await query_prometheus(container_query)
    container_count = int(float(container_results[0]["value"][1])) if container_results else 0

    # Pod count
    pod_query = 'count(count by (pod, namespace) (container_last_seen{container!="",container!="POD"}))'
    pod_results = await query_prometheus(pod_query)
    pod_count = int(float(pod_results[0]["value"][1])) if pod_results else 0

    return ClusterMetrics(
        total_cpu_cores=round(total_cpu, 2),
        total_memory_bytes=total_memory,
        used_cpu_cores=round(used_cpu, 4),
        used_memory_bytes=used_memory,
        cpu_usage_percent=round((used_cpu / total_cpu * 100) if total_cpu > 0 else 0, 2),
        memory_usage_percent=round((used_memory / total_memory * 100) if total_memory > 0 else 0, 2),
        container_count=container_count,
        pod_count=pod_count
    )


@router.get("/prometheus/app/{app_name}", response_model=AppDetailMetrics)
async def get_app_detail_metrics(
    app_name: str,
    duration: str = Query(default="1h", description="Time range: 15m, 1h, 3h, 6h, 12h, 24h"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> AppDetailMetrics:
    """Get detailed metrics for a specific app including historical data.

    Args:
        app_name: Name of the app (namespace)
        duration: Time range for historical data

    Returns:
        AppDetailMetrics with historical CPU/memory and pod info
    """
    import time

    # Parse duration to seconds
    duration_map = {
        "15m": 15 * 60,
        "1h": 60 * 60,
        "3h": 3 * 60 * 60,
        "6h": 6 * 60 * 60,
        "12h": 12 * 60 * 60,
        "24h": 24 * 60 * 60,
    }
    duration_seconds = duration_map.get(duration, 3600)

    # Calculate time range
    end_time = time.time()
    start_time = end_time - duration_seconds

    # Determine step size based on duration (aim for ~60-100 data points)
    if duration_seconds <= 3600:  # 1h or less
        step = "60s"
    elif duration_seconds <= 21600:  # 6h or less
        step = "120s"
    else:
        step = "300s"

    # Query historical CPU usage
    cpu_query = f'sum(rate(container_cpu_usage_seconds_total{{namespace="{app_name}",container!="",container!="POD"}}[5m]))'
    cpu_results = await query_prometheus_range(cpu_query, start_time, end_time, step)

    cpu_series = []
    if cpu_results:
        for ts, val in cpu_results[0].get("values", []):
            cpu_series.append(TimeSeriesPoint(timestamp=float(ts), value=float(val)))

    # Query historical memory usage
    memory_query = f'sum(container_memory_working_set_bytes{{namespace="{app_name}",container!="",container!="POD"}})'
    memory_results = await query_prometheus_range(memory_query, start_time, end_time, step)

    memory_series = []
    if memory_results:
        for ts, val in memory_results[0].get("values", []):
            memory_series.append(TimeSeriesPoint(timestamp=float(ts), value=float(val)))

    # Get current values
    current_cpu = cpu_series[-1].value if cpu_series else 0
    current_memory = int(memory_series[-1].value) if memory_series else 0

    # Get pod status
    service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
    pods = service.get_pod_status(namespace=app_name)

    return AppDetailMetrics(
        app_name=app_name,
        namespace=app_name,
        historical=AppHistoricalMetrics(
            app_name=app_name,
            namespace=app_name,
            cpu_series=cpu_series,
            memory_series=memory_series,
            cpu_usage_cores=round(current_cpu, 4),
            memory_usage_bytes=current_memory,
            memory_usage_mb=round(current_memory / (1024 * 1024), 2)
        ),
        pods=pods
    )


@router.get("/prometheus/available")
async def check_prometheus_available() -> Dict[str, Any]:
    """Check if Prometheus is available and responding.

    Returns:
        Dict with availability status
    """
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            response = await client.get(f"{PROMETHEUS_URL}/prometheus/api/v1/status/runtimeinfo")
            available = response.status_code == 200
            return {
                "available": available,
                "message": "Prometheus is available" if available else "Prometheus returned error"
            }
    except Exception as e:
        return {
            "available": False,
            "message": f"Cannot connect to Prometheus: {str(e)}"
        }


@router.get("/pods", response_model=List[PodStatus])
async def get_pod_status(
    namespace: str = Query(default="media", description="Namespace to query"),
    app: Optional[str] = Query(default=None, description="Filter by app name"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[PodStatus]:
    """Get pod status for apps in a namespace.

    Args:
        namespace: Namespace to query
        app: Optional app name filter

    Returns:
        List of pod statuses
    """
    service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
    return service.get_pod_status(namespace=namespace, app_name=app)


@router.get("/metrics", response_model=List[PodMetrics])
async def get_pod_metrics(
    namespace: str = Query(default="media", description="Namespace to query"),
    app: Optional[str] = Query(default=None, description="Filter by app name"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[PodMetrics]:
    """Get resource metrics for pods.

    Requires metrics-server to be installed in the cluster.

    Args:
        namespace: Namespace to query
        app: Optional app name filter

    Returns:
        List of pod metrics (empty if metrics-server not available)
    """
    service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
    return service.get_pod_metrics(namespace=namespace, app_name=app)


@router.get("/health/{app_name}", response_model=AppHealth)
async def get_app_health(
    app_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> AppHealth:
    """Get overall health status for an app.

    Args:
        app_name: App name
        namespace: Namespace

    Returns:
        App health status

    Raises:
        HTTPException: If health check fails
    """
    try:
        service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
        return service.get_app_health(app_name=app_name, namespace=namespace)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/endpoints/{app_name}", response_model=List[ServiceEndpoint])
async def get_service_endpoints(
    app_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[ServiceEndpoint]:
    """Get service endpoints for an app.

    Args:
        app_name: App name
        namespace: Namespace

    Returns:
        List of service endpoints

    Raises:
        HTTPException: If endpoint lookup fails
    """
    try:
        service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
        return service.get_service_endpoints(app_name=app_name, namespace=namespace)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/metrics-available", response_model=dict)
async def check_metrics_available(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Check if metrics-server is available.

    Returns:
        Dict with availability status
    """
    service = MonitoringService(k8s_client=k8s_client, catalog=catalog)
    available = service.check_metrics_server_available()
    return {
        "available": available,
        "message": "Metrics server is available" if available else "Metrics server not found"
    }
