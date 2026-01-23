"""User management API endpoints."""

import secrets
from datetime import datetime, timedelta
from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel, EmailStr
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import selectinload

from kubarr.api.dependencies import (
    get_current_active_user,
    get_current_admin_user,
    get_db,
)
from kubarr.core.models_auth import Invite, User
from kubarr.core.security import hash_password

router = APIRouter()


# Pydantic schemas
class UserBase(BaseModel):
    username: str
    email: EmailStr


class UserCreate(UserBase):
    password: str
    is_admin: bool = False


class UserUpdate(BaseModel):
    email: Optional[EmailStr] = None
    is_active: Optional[bool] = None
    is_admin: Optional[bool] = None
    is_approved: Optional[bool] = None


class UserResponse(UserBase):
    id: int
    is_active: bool
    is_admin: bool
    is_approved: bool
    created_at: datetime
    updated_at: datetime

    class Config:
        from_attributes = True


@router.get("/", response_model=List[UserResponse])
async def list_users(
    skip: int = 0,
    limit: int = 100,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """List all users (admin only).

    Args:
        skip: Number of users to skip
        limit: Maximum number of users to return
        current_user: Current admin user
        db: Database session

    Returns:
        List of users
    """
    result = await db.execute(
        select(User).offset(skip).limit(limit)
    )
    users = result.scalars().all()
    return users


@router.get("/me", response_model=UserResponse)
async def get_current_user_info(
    current_user: User = Depends(get_current_active_user)
):
    """Get current user information.

    Args:
        current_user: Current authenticated user

    Returns:
        User information
    """
    return current_user


@router.get("/pending", response_model=List[UserResponse])
async def list_pending_users(
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """List users pending approval (admin only).

    Args:
        current_user: Current admin user
        db: Database session

    Returns:
        List of pending users
    """
    result = await db.execute(
        select(User).where(User.is_approved == False)
    )
    users = result.scalars().all()
    return users


@router.post("/", response_model=UserResponse, status_code=status.HTTP_201_CREATED)
async def create_user(
    user_data: UserCreate,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Create a new user (admin only).

    Args:
        user_data: User creation data
        current_user: Current admin user
        db: Database session

    Returns:
        Created user

    Raises:
        HTTPException: If username or email already exists
    """
    # Check if username exists
    result = await db.execute(
        select(User).where(User.username == user_data.username)
    )
    if result.scalar_one_or_none():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Username already exists"
        )

    # Check if email exists
    result = await db.execute(
        select(User).where(User.email == user_data.email)
    )
    if result.scalar_one_or_none():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Email already exists"
        )

    # Create user
    new_user = User(
        username=user_data.username,
        email=user_data.email,
        hashed_password=hash_password(user_data.password),
        is_admin=user_data.is_admin,
        is_active=True,
        is_approved=True  # Admin-created users are auto-approved
    )

    db.add(new_user)
    await db.commit()
    await db.refresh(new_user)

    return new_user


# Invite schemas (must be defined before invite endpoints)
class InviteCreate(BaseModel):
    expires_in_days: Optional[int] = 7


class InviteResponse(BaseModel):
    id: int
    code: str
    created_by_username: str
    used_by_username: Optional[str] = None
    is_used: bool
    expires_at: Optional[datetime] = None
    created_at: datetime
    used_at: Optional[datetime] = None

    class Config:
        from_attributes = True


# Invite endpoints - MUST be before /{user_id} routes
@router.get("/invites", response_model=List[InviteResponse])
async def list_invites(
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """List all invites (admin only).

    Args:
        current_user: Current admin user
        db: Database session

    Returns:
        List of invites
    """
    result = await db.execute(
        select(Invite)
        .options(selectinload(Invite.created_by), selectinload(Invite.used_by))
        .order_by(Invite.created_at.desc())
    )
    invites = result.scalars().all()

    return [
        InviteResponse(
            id=invite.id,
            code=invite.code,
            created_by_username=invite.created_by.username if invite.created_by else "Unknown",
            used_by_username=invite.used_by.username if invite.used_by else None,
            is_used=invite.is_used,
            expires_at=invite.expires_at,
            created_at=invite.created_at,
            used_at=invite.used_at,
        )
        for invite in invites
    ]


@router.post("/invites", response_model=InviteResponse, status_code=status.HTTP_201_CREATED)
async def create_invite(
    invite_data: InviteCreate = None,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Create a new invite link (admin only).

    Args:
        invite_data: Invite creation data with optional expiry
        current_user: Current admin user
        db: Database session

    Returns:
        Created invite with code
    """
    if invite_data is None:
        invite_data = InviteCreate()

    # Generate a secure random code
    code = secrets.token_urlsafe(32)

    # Calculate expiration
    expires_at = None
    if invite_data.expires_in_days:
        expires_at = datetime.utcnow() + timedelta(days=invite_data.expires_in_days)

    # Create invite
    new_invite = Invite(
        code=code,
        created_by_id=current_user.id,
        expires_at=expires_at,
    )

    db.add(new_invite)
    await db.commit()
    await db.refresh(new_invite)

    return InviteResponse(
        id=new_invite.id,
        code=new_invite.code,
        created_by_username=current_user.username,
        used_by_username=None,
        is_used=new_invite.is_used,
        expires_at=new_invite.expires_at,
        created_at=new_invite.created_at,
        used_at=new_invite.used_at,
    )


@router.delete("/invites/{invite_id}")
async def delete_invite(
    invite_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Delete an invite (admin only).

    Args:
        invite_id: Invite ID
        current_user: Current admin user
        db: Database session

    Returns:
        Success message

    Raises:
        HTTPException: If invite not found
    """
    result = await db.execute(
        select(Invite).where(Invite.id == invite_id)
    )
    invite = result.scalar_one_or_none()

    if not invite:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Invite not found"
        )

    await db.delete(invite)
    await db.commit()

    return {"message": "Invite deleted"}


# User ID routes - these must come AFTER /invites routes
@router.get("/{user_id}", response_model=UserResponse)
async def get_user(
    user_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Get user by ID (admin only).

    Args:
        user_id: User ID
        current_user: Current admin user
        db: Database session

    Returns:
        User information

    Raises:
        HTTPException: If user not found
    """
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    user = result.scalar_one_or_none()

    if not user:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="User not found"
        )

    return user


@router.patch("/{user_id}", response_model=UserResponse)
async def update_user(
    user_id: int,
    user_data: UserUpdate,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Update user (admin only).

    Args:
        user_id: User ID
        user_data: User update data
        current_user: Current admin user
        db: Database session

    Returns:
        Updated user

    Raises:
        HTTPException: If user not found
    """
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    user = result.scalar_one_or_none()

    if not user:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="User not found"
        )

    # Update fields
    if user_data.email is not None:
        user.email = user_data.email
    if user_data.is_active is not None:
        user.is_active = user_data.is_active
    if user_data.is_admin is not None:
        user.is_admin = user_data.is_admin
    if user_data.is_approved is not None:
        user.is_approved = user_data.is_approved

    user.updated_at = datetime.utcnow()

    await db.commit()
    await db.refresh(user)

    return user


@router.post("/{user_id}/approve", response_model=UserResponse)
async def approve_user(
    user_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Approve a user registration (admin only).

    Args:
        user_id: User ID
        current_user: Current admin user
        db: Database session

    Returns:
        Approved user

    Raises:
        HTTPException: If user not found
    """
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    user = result.scalar_one_or_none()

    if not user:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="User not found"
        )

    user.is_approved = True
    user.is_active = True
    user.updated_at = datetime.utcnow()

    await db.commit()
    await db.refresh(user)

    return user


@router.post("/{user_id}/reject")
async def reject_user(
    user_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Reject a user registration (admin only).

    This deletes the user from the database.

    Args:
        user_id: User ID
        current_user: Current admin user
        db: Database session

    Returns:
        Success message

    Raises:
        HTTPException: If user not found
    """
    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    user = result.scalar_one_or_none()

    if not user:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="User not found"
        )

    await db.delete(user)
    await db.commit()

    return {"message": "User rejected and deleted"}


@router.delete("/{user_id}")
async def delete_user(
    user_id: int,
    current_user: User = Depends(get_current_admin_user),
    db: AsyncSession = Depends(get_db)
):
    """Delete a user (admin only).

    Args:
        user_id: User ID
        current_user: Current admin user
        db: Database session

    Returns:
        Success message

    Raises:
        HTTPException: If user not found or trying to delete self
    """
    if user_id == current_user.id:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Cannot delete yourself"
        )

    result = await db.execute(
        select(User).where(User.id == user_id)
    )
    user = result.scalar_one_or_none()

    if not user:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="User not found"
        )

    await db.delete(user)
    await db.commit()

    return {"message": "User deleted"}
