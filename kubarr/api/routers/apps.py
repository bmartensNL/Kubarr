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
    namespace: str = Query(default="media", description="Namespace to check"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> List[str]:
    """Get list of installed apps in a namespace.

    Args:
        namespace: Namespace to check

    Returns:
        List of installed app names
    """
    manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
    return manager.get_deployed_apps(namespace)


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
    namespace: str = Query(default="media", description="Namespace"),
    k8s_client: K8sClientManager = Depends(get_k8s_client),
    catalog: AppCatalog = Depends(get_app_catalog)
) -> dict:
    """Delete an installed app.

    Args:
        app_name: Name of the app to delete
        namespace: Namespace

    Returns:
        Success status

    Raises:
        HTTPException: If deletion fails
    """
    try:
        manager = DeploymentManager(k8s_client=k8s_client, catalog=catalog)
        success = manager.remove_app(app_name, namespace)
        return {"success": success, "message": f"App '{app_name}' deleted"}
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
