"""System information API router."""

from typing import List

from fastapi import APIRouter, Depends, HTTPException

from kubarr.api.dependencies import get_current_active_user, get_k8s_client
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import ClusterInfo, SystemInfo
from kubarr.core.models_auth import User
from kubarr import __version__

# Authentication via oauth2-proxy headers (X-Auth-Request-User)
router = APIRouter()


@router.get("/info", response_model=SystemInfo)
async def get_system_info(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    current_user: User = Depends(get_current_active_user)
) -> SystemInfo:
    """Get system information (requires authentication).

    Returns:
        System information including Kubarr and Kubernetes versions
    """
    import os

    k8s_version = k8s_client.get_server_version()
    in_cluster = os.getenv("KUBARR_IN_CLUSTER", "").lower() == "true"
    metrics_available = k8s_client.check_metrics_server_available()

    return SystemInfo(
        kubarr_version=__version__,
        kubernetes_version=k8s_version,
        in_cluster=in_cluster,
        metrics_server_available=metrics_available
    )


@router.get("/namespaces", response_model=List[str])
async def list_namespaces(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    current_user: User = Depends(get_current_active_user)
) -> List[str]:
    """List all namespaces in the cluster (requires authentication).

    Returns:
        List of namespace names

    Raises:
        HTTPException: If namespace listing fails
    """
    try:
        core_api = k8s_client.get_core_v1_api()
        namespaces = core_api.list_namespace()
        return [ns.metadata.name for ns in namespaces.items]
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/cluster-info", response_model=ClusterInfo)
async def get_cluster_info(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    current_user: User = Depends(get_current_active_user)
) -> ClusterInfo:
    """Get Kubernetes cluster information (requires authentication).

    Returns:
        Cluster information

    Raises:
        HTTPException: If cluster info retrieval fails
    """
    try:
        from kubernetes import client
        import os

        # Get cluster version
        version_api = client.VersionApi()
        version_info = version_api.get_code()
        k8s_version = f"{version_info.major}.{version_info.minor}"

        # Get node count
        core_api = k8s_client.get_core_v1_api()
        nodes = core_api.list_node()
        node_count = len(nodes.items)

        # Get cluster name and server URL from current context
        contexts, active_context = client.config.list_kube_config_contexts()
        if active_context:
            cluster_name = active_context.get("context", {}).get("cluster", "unknown")
        else:
            cluster_name = "in-cluster"

        # Try to get server URL
        server_url = os.getenv("KUBERNETES_SERVICE_HOST", "unknown")
        if server_url != "unknown":
            server_url = f"https://{server_url}"

        return ClusterInfo(
            name=cluster_name,
            server=server_url,
            kubernetes_version=k8s_version,
            node_count=node_count
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/health", response_model=dict)
async def health_check() -> dict:
    """Health check endpoint.

    Returns:
        Health status
    """
    return {"status": "healthy", "service": "kubarr-api"}


@router.get("/version", response_model=dict)
async def get_version() -> dict:
    """Get backend version information.

    Returns:
        Version info including commit hash and build time
    """
    import os
    return {
        "commit_hash": os.getenv("COMMIT_HASH", "unknown"),
        "build_time": os.getenv("BUILD_TIME", "unknown"),
        "version": __version__
    }
