"""Proxy router for forwarding requests to deployed apps."""

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import StreamingResponse

from kubarr.api.dependencies import get_app_catalog, get_k8s_client
from kubarr.core.app_catalog import AppCatalog
from kubarr.core.deployment_manager import DeploymentManager
from kubarr.core.k8s_client import K8sClientManager

# No authentication dependency - handled by oauth2-proxy upstream
router = APIRouter()


async def _proxy_request(app_name: str, path: str, request: Request):
    """Internal function to proxy requests to deployed apps.

    Args:
        app_name: Name of the app
        path: Path to proxy to
        request: Original request

    Returns:
        Response from the app
    """
    # Construct target URL - apps are in their own namespace with service matching app name
    # Default to port 8080 for most apps (qbittorrent, radarr, sonarr, etc.)
    target_url = f"http://{app_name}.{app_name}.svc.cluster.local:8080/{path}"

    # Get query parameters
    query_params = dict(request.query_params)

    # Get headers (excluding host and connection)
    headers = dict(request.headers)
    headers.pop("host", None)
    headers.pop("connection", None)

    # Get body for non-GET requests
    body = None
    if request.method in ["POST", "PUT", "PATCH"]:
        body = await request.body()

    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            # Forward the request
            response = await client.request(
                method=request.method,
                url=target_url,
                params=query_params,
                headers=headers,
                content=body,
                follow_redirects=True
            )

            # Return the response
            return StreamingResponse(
                iter([response.content]),
                status_code=response.status_code,
                headers=dict(response.headers),
            )

    except httpx.ConnectError:
        raise HTTPException(
            status_code=503,
            detail=f"Cannot connect to app '{app_name}'. Is it running?"
        )
    except httpx.TimeoutException:
        raise HTTPException(
            status_code=504,
            detail=f"Request to app '{app_name}' timed out"
        )
    except Exception as e:
        raise HTTPException(
            status_code=500,
            detail=f"Error proxying to app: {str(e)}"
        )


@router.api_route("/{app_name:path}/{path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"])
async def proxy_to_app(
    app_name: str,
    path: str,
    request: Request,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
):
    """Proxy requests to deployed apps.

    This handles both /apps/{app_name}/* and /{app_name}/* formats.

    Args:
        app_name: Name of the app (may include "apps/" prefix)
        path: Path to proxy to
        request: Original request
        k8s_client: Kubernetes client
        catalog: App catalog

    Returns:
        Response from the app

    Raises:
        HTTPException: If app not found in catalog
    """
    # Handle /apps/{app_name}/* format
    if app_name.startswith("apps/"):
        app_name = app_name[5:]  # Remove "apps/" prefix

    # Skip reserved paths (these should be handled by other routers or static files)
    reserved_prefixes = ["api", "auth", "oauth2", "health", "docs", "redoc", "openapi.json", "assets", "_app", "dashboard"]
    if app_name in reserved_prefixes:
        raise HTTPException(status_code=404, detail="Path not found")

    # Check if app exists in catalog
    app = catalog.get_app(app_name)
    if not app:
        # Not a known app, return 404 and let it fall through to static files
        raise HTTPException(status_code=404, detail=f"App '{app_name}' not found")

    # Check if app is actually installed
    manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
    if not manager.check_namespace_exists(app_name):
        raise HTTPException(status_code=404, detail=f"App '{app_name}' not installed")

    return await _proxy_request(app_name, path, request)
