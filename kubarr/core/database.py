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


def init_db() -> None:
    """Initialize database tables synchronously."""
    # Ensure directory exists
    db_dir = os.path.dirname(DATABASE_URL)
    if db_dir and not os.path.exists(db_dir):
        os.makedirs(db_dir, exist_ok=True)

    # Import models to register them with Base
    from kubarr.core.models_auth import (
        User, OAuth2Client, OAuth2AuthorizationCode, OAuth2Token
    )

    # Create all tables
    Base.metadata.create_all(bind=sync_engine)
    print(f"Database initialized at {DATABASE_URL}")


async def close_db() -> None:
    """Close database connections."""
    await async_engine.dispose()
