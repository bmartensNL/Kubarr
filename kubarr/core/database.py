"""Database setup and session management for Kubarr."""

import os
from typing import AsyncGenerator

from sqlalchemy import create_engine
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import sessionmaker

# Base class for models
Base = declarative_base()

# Database configuration
DATABASE_URL = os.getenv("KUBARR_DB_PATH", "/data/kubarr.db")
ASYNC_DATABASE_URL = f"sqlite+aiosqlite:///{DATABASE_URL}"
SYNC_DATABASE_URL = f"sqlite:///{DATABASE_URL}"

# Async engine for normal operations
async_engine = create_async_engine(
    ASYNC_DATABASE_URL,
    echo=False,
    connect_args={"check_same_thread": False}
)

# Sync engine for database initialization
sync_engine = create_engine(
    SYNC_DATABASE_URL,
    echo=False,
    connect_args={"check_same_thread": False}
)

# Session makers
async_session_maker = async_sessionmaker(
    async_engine,
    class_=AsyncSession,
    expire_on_commit=False
)

sync_session_maker = sessionmaker(
    sync_engine,
    autocommit=False,
    autoflush=False
)


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    """Get async database session.

    Yields:
        AsyncSession: Database session
    """
    async with async_session_maker() as session:
        try:
            yield session
            await session.commit()
        except Exception:
            await session.rollback()
            raise
        finally:
            await session.close()


def seed_default_roles(session) -> None:
    """Create default roles if they don't exist."""
    from kubarr.core.models_auth import Role, RoleAppPermission

    default_roles = [
        {
            "name": "admin",
            "description": "Full administrative access to all apps and settings",
            "is_system": True,
            "apps": []  # Empty = all apps (handled in code)
        },
        {
            "name": "viewer",
            "description": "Access to media servers and request tools",
            "is_system": True,
            "apps": ["jellyfin", "jellyseerr"]
        },
        {
            "name": "downloader",
            "description": "Access to download clients and indexers",
            "is_system": True,
            "apps": ["qbittorrent", "transmission", "deluge", "rutorrent", "sabnzbd", "jackett"]
        }
    ]

    for role_data in default_roles:
        # Check if role exists
        existing = session.query(Role).filter(Role.name == role_data["name"]).first()
        if existing:
            continue

        # Create role
        role = Role(
            name=role_data["name"],
            description=role_data["description"],
            is_system=role_data["is_system"]
        )
        session.add(role)
        session.flush()  # Get the role ID

        # Add app permissions
        for app_name in role_data["apps"]:
            perm = RoleAppPermission(role_id=role.id, app_name=app_name)
            session.add(perm)

        print(f"Created role: {role_data['name']}")

    session.commit()


def migrate_existing_users(session) -> None:
    """Assign roles to existing users based on is_admin flag."""
    from kubarr.core.models_auth import User, Role, user_roles

    admin_role = session.query(Role).filter(Role.name == "admin").first()
    viewer_role = session.query(Role).filter(Role.name == "viewer").first()

    if not admin_role or not viewer_role:
        print("Roles not found, skipping user migration")
        return

    # Get users without any roles
    users = session.query(User).all()
    for user in users:
        # Check if user already has roles
        if user.roles:
            continue

        if user.is_admin:
            user.roles.append(admin_role)
            print(f"Assigned admin role to user: {user.username}")
        else:
            user.roles.append(viewer_role)
            print(f"Assigned viewer role to user: {user.username}")

    session.commit()


def init_db() -> None:
    """Initialize database tables synchronously."""
    # Ensure directory exists
    db_dir = os.path.dirname(DATABASE_URL)
    if db_dir and not os.path.exists(db_dir):
        os.makedirs(db_dir, exist_ok=True)

    # Import models to register them with Base
    from kubarr.core.models_auth import (
        User, OAuth2Client, OAuth2AuthorizationCode, OAuth2Token, Invite,
        Role, RoleAppPermission, user_roles
    )

    # Create all tables
    Base.metadata.create_all(bind=sync_engine)
    print(f"Database initialized at {DATABASE_URL}")

    # Seed default roles and migrate users
    with sync_session_maker() as session:
        seed_default_roles(session)
        migrate_existing_users(session)


async def close_db() -> None:
    """Close database connections."""
    await async_engine.dispose()
