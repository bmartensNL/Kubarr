"""Apps management API router."""

from typing import List

from fastapi import APIRouter, Depends, HTTPException, Query

from kubarr.api.dependencies import (
    get_app_catalog,
    get_current_active_user,
    get_deployment_manager,
    get_k8s_client,
)
from kubarr.core.app_catalog import AppCatalog
from kubarr.core.deployment_manager import DeploymentManager
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.models import AppConfig, AppInfo, DeploymentRequest, DeploymentStatus
from kubarr.core.models_auth import User

# Authentication via oauth2-proxy headers (X-Auth-Request-User)
router = APIRouter(dependencies=[Depends(get_current_active_user)])


@router.get("/catalog", response_model=List[AppConfig])
async def list_catalog(catalog: AppCatalog = Depends(get_app_catalog)) -> List[AppConfig]:
    """Get all available apps in the catalog.

    Returns:
        List of available apps
    """
    return catalog.get_all_apps()


@router.get("/catalog/{app_name}", response_model=AppConfig)
async def get_app_from_catalog(
    app_name: str,
    catalog: AppCatalog = Depends(get_app_catalog)
) -> AppConfig:
    """Get a specific app from the catalog.

    Args:
        app_name: Name of the app

    Returns:
        App configuration

    Raises:
        HTTPException: If app not found
    """
    app = catalog.get_app(app_name)
    if not app:
        raise HTTPException(status_code=404, detail=f"App '{app_name}' not found")
    return app


@router.get("/installed", response_model=List[str])
async def list_installed_apps(
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[str]:
    """Get list of installed apps (all namespaces).

    Returns:
        List of installed app names
    """
    manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
    return manager.get_deployed_apps()


@router.post("/install", response_model=DeploymentStatus)
async def install_app(
    request: DeploymentRequest,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> DeploymentStatus:
    """Install a new app.

    Args:
        request: Deployment request

    Returns:
        Deployment status

    Raises:
        HTTPException: If deployment fails
    """
    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        return manager.deploy_app(request)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except RuntimeError as e:
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/{app_name}")
async def delete_app(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Delete an installed app (deletes entire namespace).

    Args:
        app_name: Name of the app to delete

    Returns:
        Success status

    Raises:
        HTTPException: If deletion fails
    """
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
    k8s_client: K8sClientManager = Depends(get_k8s_client)
) -> dict:
    """Restart an app by deleting its pods.

    Args:
        app_name: Name of the app to restart
        namespace: Namespace

    Returns:
        Success status

    Raises:
        HTTPException: If restart fails
    """
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
async def list_categories(catalog: AppCatalog = Depends(get_app_catalog)) -> List[str]:
    """Get all app categories.

    Returns:
        List of category names
    """
    return catalog.get_categories()


@router.get("/category/{category}", response_model=List[AppConfig])
async def get_apps_by_category(
    category: str,
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[AppConfig]:
    """Get all apps in a specific category.

    Args:
        category: Category name

    Returns:
        List of apps in the category
    """
    return catalog.get_apps_by_category(category)


@router.get("/{app_name}/health")
async def check_app_health(
    app_name: str,
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Check health of an installed app.

    Args:
        app_name: Name of the app

    Returns:
        Health status

    Raises:
        HTTPException: If health check fails
    """
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
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Check if an app namespace exists.

    Args:
        app_name: Name of the app

    Returns:
        Exists status
    """
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
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Get the current status of an app.

    Args:
        app_name: Name of the app

    Returns:
        Status: idle, installing, installed, or error
    """
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
