"""Monitoring API router."""

from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query

from kubarr.api.dependencies import (
    get_app_catalog,
    get_k8s_client,
    get_monitoring_service,
)
from kubarr.core.app_catalog import AppCatalog
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppHealth, PodMetrics, PodStatus, ServiceEndpoint
from kubarr.core.monitoring_service import MonitoringService

router = APIRouter()


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
