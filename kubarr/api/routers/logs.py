"""Logs API router."""

import httpx
from datetime import datetime, timedelta
from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel

from kubarr.api.dependencies import get_current_active_user, get_k8s_client, get_logs_service
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.logs_service import LogsService
from kubarr.core.models import LogEntry, LogFilter
from kubarr.core.models_auth import User

# Authentication via oauth2-proxy headers (X-Auth-Request-User)
router = APIRouter(dependencies=[Depends(get_current_active_user)])

# Loki service URL (inside cluster)
LOKI_URL = "http://loki.loki.svc.cluster.local:3100"


class LokiLogEntry(BaseModel):
    """A log entry from Loki."""
    timestamp: str
    line: str
    labels: dict


class LokiQueryResponse(BaseModel):
    """Response from Loki query."""
    streams: List[dict]
    total_entries: int


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


# ============== Loki Endpoints ==============

@router.get("/loki/labels")
async def get_loki_labels():
    """Get all available labels from Loki.

    Returns:
        List of label names
    """
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            response = await client.get(f"{LOKI_URL}/loki/api/v1/labels")
            response.raise_for_status()
            data = response.json()
            return data.get("data", [])
    except httpx.RequestError as e:
        raise HTTPException(status_code=503, detail=f"Failed to connect to Loki: {str(e)}")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/loki/label/{label}/values")
async def get_loki_label_values(label: str):
    """Get all values for a specific label from Loki.

    Args:
        label: Label name (e.g., 'namespace', 'app', 'container')

    Returns:
        List of label values
    """
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            response = await client.get(f"{LOKI_URL}/loki/api/v1/label/{label}/values")
            response.raise_for_status()
            data = response.json()
            return data.get("data", [])
    except httpx.RequestError as e:
        raise HTTPException(status_code=503, detail=f"Failed to connect to Loki: {str(e)}")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/loki/query")
async def query_loki_logs(
    query: str = Query(default='{namespace=~".+"}', description="LogQL query"),
    start: Optional[str] = Query(default=None, description="Start time (RFC3339 or Unix nanoseconds)"),
    end: Optional[str] = Query(default=None, description="End time (RFC3339 or Unix nanoseconds)"),
    limit: int = Query(default=1000, description="Maximum number of entries to return"),
    direction: str = Query(default="backward", description="Log order: 'forward' or 'backward'"),
):
    """Query logs from Loki using LogQL.

    Args:
        query: LogQL query string (e.g., '{namespace="sonarr"}')
        start: Start time for the query range
        end: End time for the query range
        limit: Maximum entries to return
        direction: Log order (backward = newest first)

    Returns:
        Log streams with entries
    """
    try:
        # Default to last hour if no time range specified
        now = datetime.utcnow()
        if not end:
            end = str(int(now.timestamp() * 1e9))
        if not start:
            start_time = now - timedelta(hours=1)
            start = str(int(start_time.timestamp() * 1e9))

        params = {
            "query": query,
            "start": start,
            "end": end,
            "limit": limit,
            "direction": direction,
        }

        async with httpx.AsyncClient(timeout=60.0) as client:
            response = await client.get(
                f"{LOKI_URL}/loki/api/v1/query_range",
                params=params
            )
            response.raise_for_status()
            data = response.json()

            # Parse Loki response
            result = data.get("data", {}).get("result", [])
            streams = []
            total_entries = 0

            for stream in result:
                labels = stream.get("stream", {})
                values = stream.get("values", [])
                entries = []

                for value in values:
                    timestamp_ns, line = value
                    # Convert nanoseconds to ISO format
                    timestamp_s = int(timestamp_ns) / 1e9
                    dt = datetime.utcfromtimestamp(timestamp_s)
                    entries.append({
                        "timestamp": dt.isoformat() + "Z",
                        "line": line,
                    })
                    total_entries += 1

                streams.append({
                    "labels": labels,
                    "entries": entries,
                })

            return {
                "streams": streams,
                "total_entries": total_entries,
            }

    except httpx.RequestError as e:
        raise HTTPException(status_code=503, detail=f"Failed to connect to Loki: {str(e)}")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/loki/namespaces")
async def get_loki_namespaces():
    """Get all namespaces that have logs in Loki.

    Returns:
        List of namespace names
    """
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            response = await client.get(f"{LOKI_URL}/loki/api/v1/label/namespace/values")
            response.raise_for_status()
            data = response.json()
            return data.get("data", [])
    except httpx.RequestError as e:
        raise HTTPException(status_code=503, detail=f"Failed to connect to Loki: {str(e)}")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
