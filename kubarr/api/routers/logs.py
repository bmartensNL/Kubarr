"""Logs API router."""

from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query

from kubarr.api.dependencies import get_current_active_user, get_k8s_client, get_logs_service
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.logs_service import LogsService
from kubarr.core.models import LogEntry, LogFilter
from kubarr.core.models_auth import User

# Authentication via oauth2-proxy headers (X-Auth-Request-User)
router = APIRouter(dependencies=[Depends(get_current_active_user)])


@router.get("/{pod_name}", response_model=List[LogEntry])
async def get_pod_logs(
    pod_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    container: Optional[str] = Query(default=None, description="Container name"),
    tail: int = Query(default=100, description="Number of lines"),
    k8s_client: K8sClientManager = Depends(get_k8s_client)
) -> List[LogEntry]:
    """Get logs from a specific pod.

    Args:
        pod_name: Pod name
        namespace: Namespace
        container: Optional container name
        tail: Number of lines to return

    Returns:
        List of log entries

    Raises:
        HTTPException: If log retrieval fails
    """
    try:
        service = LogsService(k8s_client=k8s_client)
        log_filter = LogFilter(
            namespace=namespace,
            pod_name=pod_name,
            container=container,
            tail_lines=tail
        )
        return service.get_logs(log_filter)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/app/{app_name}", response_model=List[LogEntry])
async def get_app_logs(
    app_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    tail: int = Query(default=100, description="Number of lines per pod"),
    k8s_client: K8sClientManager = Depends(get_k8s_client)
) -> List[LogEntry]:
    """Get logs from all pods of an app.

    Args:
        app_name: App name
        namespace: Namespace
        tail: Number of lines to return per pod

    Returns:
        List of log entries from all pods

    Raises:
        HTTPException: If log retrieval fails
    """
    try:
        service = LogsService(k8s_client=k8s_client)
        log_filter = LogFilter(
            namespace=namespace,
            app_name=app_name,
            tail_lines=tail
        )
        return service.get_logs(log_filter)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/raw/{pod_name}", response_model=str)
async def get_raw_pod_logs(
    pod_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    container: Optional[str] = Query(default=None, description="Container name"),
    tail: int = Query(default=100, description="Number of lines"),
    k8s_client: K8sClientManager = Depends(get_k8s_client)
) -> str:
    """Get raw logs from a pod as plain text.

    Args:
        pod_name: Pod name
        namespace: Namespace
        container: Optional container name
        tail: Number of lines

    Returns:
        Raw log string

    Raises:
        HTTPException: If log retrieval fails
    """
    try:
        service = LogsService(k8s_client=k8s_client)
        return service.get_pod_logs(
            pod_name=pod_name,
            namespace=namespace,
            container=container,
            tail=tail
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
