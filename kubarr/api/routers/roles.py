"""Role management API endpoints."""

from datetime import datetime
from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from kubarr.api.dependencies import (
    get_current_active_user,
    get_current_admin_user,
    get_db,
)
from kubarr.core.models_auth import Role, RoleAppPermission, User

router = APIRouter()


# Pydantic schemas
class RoleAppPermissionResponse(BaseModel):
    id: int
    app_name: str

    class Config:
        from_attributes = True


class RoleBase(BaseModel):
    name: str
    description: Optional[str] = None


class RoleCreate(RoleBase):
    app_names: List[str] = []


class RoleUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None


class RoleResponse(RoleBase):
    id: int
    is_system: bool
    created_at: datetime
    app_permissions: List[RoleAppPermissionResponse] = []

    class Config:
        from_attributes = True


class RoleWithAppsResponse(BaseModel):
    id: int
    name: str
    description: Optional[str]
    is_system: bool
    created_at: datetime
    app_names: List[str]

    class Config:
        from_attributes = True


class SetRoleAppsRequest(BaseModel):
    app_names: List[str]


@router.get("/", response_model=List[RoleWithAppsResponse])
async def list_roles(
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db)
):
    """List all roles.

    Args:
        current_user: Current authenticated user
        db: Database session

    Returns:
        List of roles with their app permissions
    """
    result = await db.execute(
        select(Role).options(selectinload(Role.app_permissions))
    )
    roles = result.scalars().all()

    return [
        RoleWithAppsResponse(
            id=role.id,
            name=role.name,
            description=role.description,
            is_system=role.is_system,
            created_at=role.created_at,
            app_names=[perm.app_name for perm in role.app_permissions]
        )
        for role in roles
    ]


@router.get("/{role_id}", response_model=RoleWithAppsResponse)
async def get_role(
    role_id: int,
    current_user: User = Depends(get_current_active_user),
    db: AsyncSession = Depends(get_db)
):
    """Get role by ID.

    Args:
        role_id: Role ID
        current_user: Current authenticated user
        db: Database session

    Returns:
        Role with app permissions

    Raises:
        HTTPException: If role not found
    """
    result = await db.execute(
        select(Role)
        .options(selectinload(Role.app_permissions))
        .where(Role.id == role_id)
    )
    role = result.scalar_one_or_none()

    if not role:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Role not found"
        )

    return RoleWithAppsResponse(
        id=role.id,
        name=role.name,
        description=role.description,
        is_system=role.is_system,
        created_at=role.created_at,
        app_names=[perm.app_name for perm in role.app_permissions]
    )


@router.post("/", response_model=RoleWithAppsResponse, status_code=status.HTTP_201_CREATED)
async def create_role(
    role_data: RoleCreate,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Create a new role (admin only).

    Args:
        role_data: Role creation data
        current_user: Current admin user
        db: Database session

    Returns:
        Created role

    Raises:
        HTTPException: If role name already exists
    """
    # Check if role name exists
    result = await db.execute(
        select(Role).where(Role.name == role_data.name)
    )
    if result.scalar_one_or_none():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Role name already exists"
        )

    # Create role
    new_role = Role(
        name=role_data.name,
        description=role_data.description,
        is_system=False  # User-created roles are not system roles
    )
    db.add(new_role)
    await db.flush()

    # Add app permissions
    for app_name in role_data.app_names:
        perm = RoleAppPermission(role_id=new_role.id, app_name=app_name)
        db.add(perm)

    await db.commit()
    await db.refresh(new_role)

    # Reload with permissions
    result = await db.execute(
        select(Role)
        .options(selectinload(Role.app_permissions))
        .where(Role.id == new_role.id)
    )
    role = result.scalar_one()

    return RoleWithAppsResponse(
        id=role.id,
        name=role.name,
        description=role.description,
        is_system=role.is_system,
        created_at=role.created_at,
        app_names=[perm.app_name for perm in role.app_permissions]
    )


@router.patch("/{role_id}", response_model=RoleWithAppsResponse)
async def update_role(
    role_id: int,
    role_data: RoleUpdate,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Update role (admin only).

    Args:
        role_id: Role ID
        role_data: Role update data
        current_user: Current admin user
        db: Database session

    Returns:
        Updated role

    Raises:
        HTTPException: If role not found or trying to rename system role
    """
    result = await db.execute(
        select(Role)
        .options(selectinload(Role.app_permissions))
        .where(Role.id == role_id)
    )
    role = result.scalar_one_or_none()

    if not role:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Role not found"
        )

    # Prevent renaming system roles
    if role.is_system and role_data.name is not None and role_data.name != role.name:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Cannot rename system roles"
        )

    # Check for duplicate name
    if role_data.name is not None and role_data.name != role.name:
        existing = await db.execute(
            select(Role).where(Role.name == role_data.name)
        )
        if existing.scalar_one_or_none():
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Role name already exists"
            )

    # Update fields
    if role_data.name is not None:
        role.name = role_data.name
    if role_data.description is not None:
        role.description = role_data.description

    await db.commit()
    await db.refresh(role)

    return RoleWithAppsResponse(
        id=role.id,
        name=role.name,
        description=role.description,
        is_system=role.is_system,
        created_at=role.created_at,
        app_names=[perm.app_name for perm in role.app_permissions]
    )


@router.delete("/{role_id}")
async def delete_role(
    role_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Delete a role (admin only).

    System roles cannot be deleted.

    Args:
        role_id: Role ID
        current_user: Current admin user
        db: Database session

    Returns:
        Success message

    Raises:
        HTTPException: If role not found or is a system role
    """
    result = await db.execute(
        select(Role).where(Role.id == role_id)
    )
    role = result.scalar_one_or_none()

    if not role:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Role not found"
        )

    if role.is_system:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Cannot delete system roles"
        )

    await db.delete(role)
    await db.commit()

    return {"message": "Role deleted"}


@router.put("/{role_id}/apps", response_model=RoleWithAppsResponse)
async def set_role_apps(
    role_id: int,
    apps_data: SetRoleAppsRequest,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Set app permissions for a role (admin only).

    This replaces all existing app permissions for the role.

    Args:
        role_id: Role ID
        apps_data: List of app names
        current_user: Current admin user
        db: Database session

    Returns:
        Updated role with new permissions

    Raises:
        HTTPException: If role not found
    """
    result = await db.execute(
        select(Role)
        .options(selectinload(Role.app_permissions))
        .where(Role.id == role_id)
    )
    role = result.scalar_one_or_none()

    if not role:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Role not found"
        )

    # Delete existing permissions
    for perm in role.app_permissions:
        await db.delete(perm)

    # Add new permissions
    for app_name in apps_data.app_names:
        perm = RoleAppPermission(role_id=role.id, app_name=app_name)
        db.add(perm)

    await db.commit()

    # Reload with new permissions
    result = await db.execute(
        select(Role)
        .options(selectinload(Role.app_permissions))
        .where(Role.id == role_id)
    )
    role = result.scalar_one()

    return RoleWithAppsResponse(
        id=role.id,
        name=role.name,
        description=role.description,
        is_system=role.is_system,
        created_at=role.created_at,
        app_names=[perm.app_name for perm in role.app_permissions]
    )
