"""Apps management API router."""

import os
import secrets
from pathlib import Path
from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query, status
from fastapi.responses import FileResponse
from pydantic import BaseModel
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.dependencies import (
    get_app_catalog,
    get_current_active_user,
    get_db,
    get_deployment_manager,
    get_k8s_client,
)
from kubarr.core.app_catalog import AppCatalog, CHARTS_DIR
from kubarr.core.deployment_manager import DeploymentManager
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppConfig, AppInfo, DeploymentRequest, DeploymentStatus
from kubarr.core.models_auth import User
from kubarr.core.oauth2_service import OAuth2Service
from kubarr.core.setup import get_storage_path

# Apps that support OAuth integration with kubarr-dashboard
OAUTH_ENABLED_APPS = {"jellyseerr"}

router = APIRouter()


def filter_apps_by_permission(apps: List[AppConfig], user: User) -> List[AppConfig]:
    """Filter apps to only those the user can access.

    Args:
        apps: List of apps to filter
        user: Current user

    Returns:
        Filtered list of apps
    """
    allowed_apps = user.get_allowed_apps()
    if allowed_apps is None:  # Admin has access to all apps
        return apps
    return [app for app in apps if app.name in allowed_apps]


def check_app_permission(app_name: str, user: User) -> None:
    """Check if user has permission to access an app.

    Args:
        app_name: Name of the app
        user: Current user

    Raises:
        HTTPException: If user doesn't have permission
    """
    if not user.can_access_app(app_name):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail=f"You don't have permission to access {app_name}"
        )


@router.get("/catalog", response_model=List[AppConfig])
async def list_catalog(
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> List[AppConfig]:
    """Get all available apps in the catalog.

    Returns apps filtered by user's role permissions.

    Returns:
        List of available apps
    """
    all_apps = catalog.get_all_apps()
    return filter_apps_by_permission(all_apps, current_user)


@router.get("/catalog/{app_name}", response_model=AppConfig)
async def get_app_from_catalog(
    app_name: str,
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> AppConfig:
    """Get a specific app from the catalog.

    Args:
        app_name: Name of the app

    Returns:
        App configuration

    Raises:
        HTTPException: If app not found or no permission
    """
    check_app_permission(app_name, current_user)
    app = catalog.get_app(app_name)
    if not app:
        raise HTTPException(status_code=404, detail=f"App '{app_name}' not found")
    return app


@router.get("/catalog/{app_name}/icon")
async def get_app_icon(app_name: str) -> FileResponse:
    """Get the icon for an app.

    This endpoint does not require authentication so icons can be
    loaded in img tags without additional auth headers.

    Args:
        app_name: Name of the app

    Returns:
        SVG icon file

    Raises:
        HTTPException: If icon not found
    """
    # Validate app name to prevent path traversal
    if ".." in app_name or "/" in app_name or "\\" in app_name:
        raise HTTPException(status_code=400, detail="Invalid app name")

    icon_path = CHARTS_DIR / app_name / "icon.svg"

    if not icon_path.exists():
        raise HTTPException(status_code=404, detail=f"Icon not found for app '{app_name}'")

    return FileResponse(
        path=str(icon_path),
        media_type="image/svg+xml",
        headers={
            "Cache-Control": "public, max-age=604800, immutable",  # Cache for 7 days, immutable
            "Vary": "Accept-Encoding",
        },
    )


@router.get("/installed", response_model=List[str])
async def list_installed_apps(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> List[str]:
    """Get list of installed apps (all namespaces).

    Returns apps filtered by user's role permissions.

    Returns:
        List of installed app names
    """
    manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
    all_installed = manager.get_deployed_apps()

    # Filter by user permissions
    allowed_apps = current_user.get_allowed_apps()
    if allowed_apps is None:  # Admin has access to all apps
        return all_installed
    return [app for app in all_installed if app in allowed_apps]


@router.post("/install", response_model=DeploymentStatus)
async def install_app(
    request: DeploymentRequest,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db)
) -> DeploymentStatus:
    """Install a new app.

    Args:
        request: Deployment request
        db: Database session for getting storage path

    Returns:
        Deployment status

    Raises:
        HTTPException: If deployment fails or no permission
    """
    check_app_permission(request.app_name, current_user)
    try:
        # Get storage path from database to inject into deployment
        storage_path = await get_storage_path(db)

        # Initialize custom config if not provided
        if request.custom_config is None:
            request.custom_config = {}

        # Create OAuth client for apps that support it
        if request.app_name in OAUTH_ENABLED_APPS:
            oauth_config = await _create_oauth_client_for_app(db, request.app_name)
            request.custom_config.update(oauth_config)

        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        return manager.deploy_app(request, storage_path=storage_path)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e))


async def _create_oauth_client_for_app(db: AsyncSession, app_name: str) -> dict:
    """Create OAuth client for an app and return Helm values.

    Args:
        db: Database session
        app_name: Name of the app

    Returns:
        Dictionary of Helm values for OAuth configuration
    """
    oauth_service = OAuth2Service(db)

    # Check if client already exists
    client_id = f"{app_name}-oauth"
    existing_client = await oauth_service.get_client(client_id)

    if existing_client:
        # Client already exists, generate new secret and update
        client_secret = secrets.token_urlsafe(32)
        from kubarr.core.security import hash_client_secret
        existing_client.client_secret_hash = hash_client_secret(client_secret)
        await db.commit()
    else:
        # Create new client
        client_secret = secrets.token_urlsafe(32)
        redirect_uris = [
            f"http://localhost:8080/{app_name}/api/v1/auth/oidc-callback",
            f"http://{app_name}.{app_name}.svc.cluster.local:5055/api/v1/auth/oidc-callback",
        ]

        await oauth_service.create_client(
            client_id=client_id,
            client_secret=client_secret,
            name=f"{app_name.title()} OAuth Client",
            redirect_uris=redirect_uris,
        )

    # Return Helm values for OAuth configuration
    return {
        f"{app_name}.oauth.enabled": "true",
        f"{app_name}.oauth.clientId": client_id,
        f"{app_name}.oauth.clientSecret": client_secret,
        f"{app_name}.oauth.issuerUrl": "http://kubarr-dashboard.kubarr.svc.cluster.local:8080",
    }


@router.delete("/{app_name}")
async def delete_app(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> dict:
    """Delete an installed app (deletes entire namespace).

    Args:
        app_name: Name of the app to delete

    Returns:
        Success status

    Raises:
        HTTPException: If deletion fails, no permission, or system app
    """
    check_app_permission(app_name, current_user)

    # Check if this is a system app
    app = catalog.get_app(app_name)
    if app and app.is_system:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail=f"Cannot delete system app '{app_name}'"
        )

    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        success = manager.remove_app(app_name)
        return {"success": success, "message": f"App '{app_name}' deletion initiated", "status": "deleting"}
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.post("/{app_name}/restart")
async def restart_app(
    app_name: str,
    namespace: str = Query(default="media", description="Namespace"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    current_user: User = Depends(get_current_active_user)
) -> dict:
    """Restart an app by deleting its pods.

    Args:
        app_name: Name of the app to restart
        namespace: Namespace

    Returns:
        Success status

    Raises:
        HTTPException: If restart fails or no permission
    """
    check_app_permission(app_name, current_user)
    try:
        core_api = k8s_client.get_core_v1_api()

        # Delete all pods with the app label
        pods = core_api.list_namespaced_pod(
            namespace=namespace,
            label_selector=f"app={app_name}"
        )

        deleted_count = 0
        for pod in pods.items:
            core_api.delete_namespaced_pod(
                name=pod.metadata.name,
                namespace=namespace
            )
            deleted_count += 1

        return {
            "success": True,
            "message": f"Restarted {deleted_count} pod(s) for app '{app_name}'"
        }
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/categories", response_model=List[str])
async def list_categories(
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> List[str]:
    """Get all app categories.

    Returns:
        List of category names
    """
    return catalog.get_categories()


@router.get("/category/{category}", response_model=List[AppConfig])
async def get_apps_by_category(
    category: str,
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> List[AppConfig]:
    """Get all apps in a specific category.

    Returns apps filtered by user's role permissions.

    Args:
        category: Category name

    Returns:
        List of apps in the category
    """
    apps = catalog.get_apps_by_category(category)
    return filter_apps_by_permission(apps, current_user)


@router.get("/{app_name}/health")
async def check_app_health(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> dict:
    """Check health of an installed app.

    Args:
        app_name: Name of the app

    Returns:
        Health status

    Raises:
        HTTPException: If health check fails or no permission
    """
    check_app_permission(app_name, current_user)
    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        health = manager.check_namespace_health(app_name)
        return health
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{app_name}/exists")
async def check_app_exists(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> dict:
    """Check if an app namespace exists.

    Args:
        app_name: Name of the app

    Returns:
        Exists status

    Raises:
        HTTPException: If no permission
    """
    check_app_permission(app_name, current_user)
    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        exists = manager.check_namespace_exists(app_name)
        return {"exists": exists}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/{app_name}/status")
async def get_app_status(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog),
    current_user: User = Depends(get_current_active_user)
) -> dict:
    """Get the current status of an app.

    Args:
        app_name: Name of the app

    Returns:
        Status: idle, installing, installed, or error

    Raises:
        HTTPException: If no permission
    """
    check_app_permission(app_name, current_user)
    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)

        # Check if namespace exists
        exists = manager.check_namespace_exists(app_name)

        if not exists:
            return {"state": "idle", "message": "Not installed"}

        # Namespace exists, check health
        health = manager.check_namespace_health(app_name)

        if health["status"] == "healthy":
            return {"state": "installed", "message": "Running"}
        elif health["status"] == "no_deployments":
            return {"state": "idle", "message": "No deployments found"}
        else:
            # Deployments exist but not healthy yet - could be installing
            return {"state": "installing", "message": health.get("message", "Waiting for deployments to be ready")}

    except Exception as e:
        return {"state": "error", "message": str(e)}
