"""Dependency injection for FastAPI."""

from functools import lru_cache
from typing import Generator, Optional

from fastapi import Depends, HTTPException, status, Request
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.config import Settings, settings
from kubarr.core.app_catalog import AppCatalog
from kubarr.core.database import get_db
from kubarr.core.deployment_manager import DeploymentManager
from kubarr.core.k8s_client import K8sClientManager
from kubarr.core.logs_service import LogsService
from kubarr.core.models_auth import User
from kubarr.core.monitoring_service import MonitoringService


@lru_cache()
def get_settings() -> Settings:
    """Get application settings.

    Returns:
        Settings instance
    """
    return settings


def get_k8s_client() -> Generator[K8sClientManager, None, None]:
    """Get Kubernetes client.

    Yields:
        K8sClientManager instance
    """
    client = K8sClientManager(
        kubeconfig_path=settings.kubeconfig_path,
        in_cluster=settings.in_cluster
    )
    try:
        yield client
    finally:
        # Cleanup if needed
        pass


def get_app_catalog() -> AppCatalog:
    """Get application catalog.

    Returns:
        AppCatalog instance
    """
    return AppCatalog()


def get_deployment_manager(
    k8s_client: K8sClientManager,
    catalog: AppCatalog
) -> DeploymentManager:
    """Get deployment manager.

    Args:
        k8s_client: Kubernetes client
        catalog: App catalog

    Returns:
        DeploymentManager instance
    """
    return DeploymentManager(k8s_client=k8s_client, catalog=catalog)


def get_monitoring_service(
    k8s_client: K8sClientManager,
    catalog: AppCatalog
) -> MonitoringService:
    """Get monitoring service.

    Args:
        k8s_client: Kubernetes client
        catalog: App catalog

    Returns:
        MonitoringService instance
    """
    return MonitoringService(k8s_client=k8s_client, catalog=catalog)


def get_logs_service(k8s_client: K8sClientManager) -> LogsService:
    """Get logs service.

    Args:
        k8s_client: Kubernetes client

    Returns:
        LogsService instance
    """
    return LogsService(k8s_client=k8s_client)


async def get_current_user(
    request: Request,
    db: AsyncSession = Depends(get_db)
) -> Optional[User]:
    """Get current authenticated user from internal headers.

    oauth2-proxy sets X-Auth-Request-User/Email headers for authenticated
    requests. These headers are stripped from responses by nginx so the
    browser never sees them.

    Args:
        request: FastAPI request object
        db: Database session

    Returns:
        User or None if not authenticated
    """
    # Get user identity from oauth2-proxy headers (internal only)
    user_email = request.headers.get("X-Auth-Request-Email")
    user_name = request.headers.get("X-Auth-Request-User")

    # Try email first, then username
    identifier = user_email or user_name

    if not identifier:
        return None

    # Get user from database by email or username
    result = await db.execute(
        select(User).where((User.email == identifier) | (User.username == identifier))
    )
    user = result.scalar_one_or_none()

    return user


async def get_current_active_user(
    current_user: Optional[User] = Depends(get_current_user)
) -> User:
    """Get current active user (required).

    Args:
        current_user: Current user from token

    Returns:
        User

    Raises:
        HTTPException: If user is not authenticated or not active
    """
    if current_user is None:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Not authenticated",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if not current_user.is_active:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Inactive user"
        )

    if not current_user.is_approved:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User account not approved"
        )

    return current_user


async def get_current_admin_user(
    current_user: User = Depends(get_current_active_user)
) -> User:
    """Get current admin user (required).

    Args:
        current_user: Current active user

    Returns:
        User

    Raises:
        HTTPException: If user is not an admin
    """
    # Check both legacy is_admin flag and admin role
    if not current_user.has_admin_role():
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Not enough permissions"
        )

    return current_user


def require_app_access(app_name: str):
    """Dependency factory to check if user can access a specific app.

    Args:
        app_name: Name of the app to check access for

    Returns:
        Dependency function that validates access
    """
    async def check_access(current_user: User = Depends(get_current_active_user)) -> User:
        if not current_user.can_access_app(app_name):
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail=f"You don't have permission to access {app_name}"
            )
        return current_user
    return check_access


def get_optional_current_user(
    current_user: Optional[User] = Depends(get_current_user)
) -> Optional[User]:
    """Get current user if authenticated, otherwise None.

    This is for endpoints that work both authenticated and unauthenticated.

    Args:
        current_user: Current user from token

    Returns:
        User or None
    """
    return current_user
