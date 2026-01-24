"""Setup wizard for initial Kubarr configuration."""

import logging
import os
from pathlib import Path
from typing import Tuple

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.config import settings
from kubarr.core.models_auth import OAuth2Client, SystemSettings, User
from kubarr.core.security import (
    generate_random_string,
    generate_secure_password,
    hash_client_secret,
    hash_password,
)

logger = logging.getLogger(__name__)

# Folder structure to create under root storage path
STORAGE_FOLDERS = [
    "downloads",
    "downloads/qbittorrent",
    "downloads/qbittorrent/incomplete",
    "downloads/transmission",
    "downloads/transmission/incomplete",
    "downloads/transmission/watch",
    "downloads/deluge",
    "downloads/rutorrent",
    "downloads/sabnzbd",
    "downloads/nzbget",
    "media",
    "media/movies",
    "media/tv",
    "media/music",
]


def validate_storage_path(path: str) -> Tuple[bool, str | None]:
    """Validate that a storage path is usable.

    Args:
        path: Storage path to validate

    Returns:
        Tuple of (is_valid, error_message)
    """
    if not path:
        return False, "Storage path cannot be empty"

    # Normalize and resolve the path
    try:
        storage_path = Path(path).resolve()
    except Exception as e:
        return False, f"Invalid path format: {str(e)}"

    # Check if path exists
    if not storage_path.exists():
        return False, f"Path does not exist: {path}"

    # Check if it's a directory
    if not storage_path.is_dir():
        return False, f"Path is not a directory: {path}"

    # Check if we can write to it
    test_file = storage_path / ".kubarr_test"
    try:
        test_file.touch()
        test_file.unlink()
    except PermissionError:
        return False, f"No write permission for path: {path}"
    except Exception as e:
        return False, f"Cannot write to path: {str(e)}"

    return True, None


def create_storage_structure(root_path: str, uid: int = 1000, gid: int = 1000) -> None:
    """Create the folder structure for media storage.

    Args:
        root_path: Root storage path (node path for DB, or mount path for container)
        uid: User ID for ownership
        gid: Group ID for ownership

    Raises:
        OSError: If folder creation fails

    Note:
        Inside a container, use KUBARR_STORAGE_PATH env var which points to
        where the hostPath volume is mounted. The root_path argument is the
        node path, which is stored in the database for Helm deployments.
    """
    # In container, use the mount path from env var instead of raw node path
    mount_path = os.environ.get("KUBARR_STORAGE_PATH")
    if mount_path and Path(mount_path).exists():
        base_path = Path(mount_path)
    else:
        # Outside container or mount not ready - try raw path
        base_path = Path(root_path)

    for folder in STORAGE_FOLDERS:
        folder_path = base_path / folder
        folder_path.mkdir(parents=True, exist_ok=True)
        # Set permissions to 775 (rwxrwxr-x)
        os.chmod(folder_path, 0o775)
        # Try to set ownership (may fail if not root)
        try:
            os.chown(folder_path, uid, gid)
        except (PermissionError, OSError):
            # Ignore ownership errors - may not be running as root
            pass


async def save_storage_settings(db: AsyncSession, storage_path: str) -> SystemSettings:
    """Save storage path to system settings.

    Args:
        db: Database session
        storage_path: Root storage path

    Returns:
        Created/updated SystemSettings entry
    """
    # Check if setting already exists
    result = await db.execute(
        select(SystemSettings).where(SystemSettings.key == "storage_path")
    )
    existing = result.scalar_one_or_none()

    if existing:
        existing.value = storage_path
        setting = existing
    else:
        setting = SystemSettings(
            key="storage_path",
            value=storage_path,
            description="Root storage path for media apps (hostPath)"
        )
        db.add(setting)

    await db.commit()
    await db.refresh(setting)
    return setting


async def get_storage_path(db: AsyncSession) -> str | None:
    """Get configured storage path from database.

    Args:
        db: Database session

    Returns:
        Storage path or None if not configured
    """
    result = await db.execute(
        select(SystemSettings).where(SystemSettings.key == "storage_path")
    )
    setting = result.scalar_one_or_none()
    return setting.value if setting else None


async def is_setup_required(db: AsyncSession) -> bool:
    """Check if initial setup is required.

    Args:
        db: Database session

    Returns:
        True if setup is required (no admin user exists)
    """
    result = await db.execute(select(User).where(User.is_admin == True))
    admin_user = result.scalar_one_or_none()

    return admin_user is None


async def create_admin_user(
    db: AsyncSession, username: str, email: str, password: str
) -> User:
    """Create the initial admin user.

    Args:
        db: Database session
        username: Admin username
        email: Admin email
        password: Admin password

    Returns:
        Created admin user

    Raises:
        ValueError: If admin user already exists
    """
    # Check if admin already exists
    result = await db.execute(select(User).where(User.is_admin == True))
    if result.scalar_one_or_none():
        raise ValueError("Admin user already exists")

    # Create admin user
    admin_user = User(
        username=username,
        email=email,
        hashed_password=hash_password(password),
        is_admin=True,
        is_active=True,
        is_approved=True,
    )

    db.add(admin_user)
    await db.commit()
    await db.refresh(admin_user)

    return admin_user


async def create_oauth2_proxy_client(
    db: AsyncSession, redirect_uris: list[str], client_secret: str | None = None
) -> tuple[OAuth2Client, str]:
    """Create OAuth2 client for oauth2-proxy.

    Args:
        db: Database session
        redirect_uris: List of redirect URIs
        client_secret: Client secret (generated if not provided)

    Returns:
        Tuple of (OAuth2Client, plain_client_secret)

    Raises:
        ValueError: If client already exists
    """
    client_id = "oauth2-proxy"

    # Check if client already exists
    result = await db.execute(
        select(OAuth2Client).where(OAuth2Client.client_id == client_id)
    )
    existing_client = result.scalar_one_or_none()

    if existing_client:
        raise ValueError("OAuth2 proxy client already exists")

    # Generate client secret if not provided
    if not client_secret:
        client_secret = generate_random_string(32)

    # Create client
    import json

    client = OAuth2Client(
        client_id=client_id,
        client_secret_hash=hash_client_secret(client_secret),
        name="OAuth2 Proxy",
        redirect_uris=json.dumps(redirect_uris),
    )

    db.add(client)
    await db.commit()
    await db.refresh(client)

    return client, client_secret


async def initialize_setup(
    db: AsyncSession,
    admin_username: str,
    admin_email: str,
    admin_password: str,
    storage_path: str,
    base_url: str | None = None,
    oauth2_client_secret: str | None = None,
) -> dict:
    """Run the complete initial setup.

    Args:
        db: Database session
        admin_username: Admin username
        admin_email: Admin email
        admin_password: Admin password
        storage_path: Root storage path for media apps
        base_url: Base URL for the application (optional, for OAuth2)
        oauth2_client_secret: OAuth2 client secret (generated if not provided)

    Returns:
        Dictionary with setup results

    Raises:
        ValueError: If setup has already been completed
        OSError: If storage directory creation fails
    """
    # Check if setup is required
    if not await is_setup_required(db):
        raise ValueError("Setup has already been completed")

    # Create storage folder structure first
    create_storage_structure(storage_path)

    # Save storage path to database
    await save_storage_settings(db, storage_path)

    # Create admin user
    admin_user = await create_admin_user(db, admin_username, admin_email, admin_password)

    # Create oauth2-proxy client if base_url is provided
    oauth2_result = None
    if base_url:
        redirect_uris = [
            f"{base_url}/oauth2/callback",
            f"{base_url}/oauth/callback",  # Alternative
        ]

        client, plain_secret = await create_oauth2_proxy_client(
            db, redirect_uris, oauth2_client_secret
        )

        # Store the plain secret in SystemSettings for later sync
        secret_setting = SystemSettings(
            key="oauth2_client_secret",
            value=plain_secret,
            description="OAuth2-proxy client secret (for syncing to Kubernetes)"
        )
        db.add(secret_setting)
        await db.commit()

        # Sync credentials to Kubernetes secret
        sync_oauth2_credentials_to_k8s(
            client_id=client.client_id,
            client_secret=plain_secret
        )

        oauth2_result = {
            "client_id": client.client_id,
            "client_secret": plain_secret,
            "redirect_uris": redirect_uris,
        }

    return {
        "admin_user": {
            "id": admin_user.id,
            "username": admin_user.username,
            "email": admin_user.email,
        },
        "storage": {
            "path": storage_path,
            "folders_created": STORAGE_FOLDERS,
        },
        "oauth2_client": oauth2_result,
    }


async def get_setup_status(db: AsyncSession) -> dict:
    """Get current setup status.

    Args:
        db: Database session

    Returns:
        Dictionary with setup status
    """
    # Check for admin user
    result = await db.execute(select(User).where(User.is_admin == True))
    admin_user = result.scalar_one_or_none()

    # Check for oauth2-proxy client
    result = await db.execute(
        select(OAuth2Client).where(OAuth2Client.client_id == "oauth2-proxy")
    )
    oauth2_client = result.scalar_one_or_none()

    # Check for storage configuration
    storage_path = await get_storage_path(db)

    return {
        "setup_required": admin_user is None,
        "admin_user_exists": admin_user is not None,
        "oauth2_client_exists": oauth2_client is not None,
        "storage_configured": storage_path is not None,
    }


def generate_initial_credentials() -> dict:
    """Generate initial admin credentials.

    Returns:
        Dictionary with generated credentials
    """
    admin_password = generate_secure_password(16)
    client_secret = generate_random_string(32)

    return {
        "admin_username": settings.admin_username,
        "admin_email": settings.admin_email,
        "admin_password": admin_password,
        "client_secret": client_secret,
    }


def sync_oauth2_credentials_to_k8s(
    client_id: str,
    client_secret: str,
    cookie_secret: str | None = None,
    namespace: str = "kubarr-system"
) -> bool:
    """Sync OAuth2 credentials to Kubernetes secret.

    This creates/updates a Kubernetes secret that oauth2-proxy reads from,
    ensuring the credentials always match what's in the database.

    Args:
        client_id: OAuth2 client ID
        client_secret: OAuth2 client secret (plain text)
        cookie_secret: Cookie encryption secret (generated if not provided)
        namespace: Target namespace

    Returns:
        True if sync was successful
    """
    import base64
    import secrets as py_secrets

    try:
        from kubarr.core.k8s_client import K8sClientManager

        # Generate cookie secret if not provided
        # oauth2-proxy requires exactly 16, 24, or 32 bytes for AES
        # We generate 32 random bytes and base64 encode them
        if not cookie_secret:
            cookie_bytes = py_secrets.token_bytes(32)
            cookie_secret = base64.urlsafe_b64encode(cookie_bytes).decode()

        k8s = K8sClientManager(in_cluster=True)
        result = k8s.sync_oauth2_proxy_secret(
            client_id=client_id,
            client_secret=client_secret,
            cookie_secret=cookie_secret,
            namespace=namespace
        )

        if result:
            logger.info(f"OAuth2 credentials synced to Kubernetes secret in {namespace}")
        else:
            logger.warning("Failed to sync OAuth2 credentials to Kubernetes")

        return result

    except Exception as e:
        logger.error(f"Error syncing OAuth2 credentials: {e}")
        return False


async def sync_oauth2_client_on_startup(db: AsyncSession) -> bool:
    """Sync OAuth2 client credentials to Kubernetes on application startup.

    This ensures oauth2-proxy always has the correct credentials, even if
    the secret was deleted or the pod was restarted.

    Args:
        db: Database session

    Returns:
        True if sync was successful or not needed
    """
    # Get oauth2-proxy client from database
    result = await db.execute(
        select(OAuth2Client).where(OAuth2Client.client_id == "oauth2-proxy")
    )
    client = result.scalar_one_or_none()

    if not client:
        logger.info("No oauth2-proxy client found - skipping credential sync")
        return True

    # Get stored client secret from system settings
    result = await db.execute(
        select(SystemSettings).where(SystemSettings.key == "oauth2_client_secret")
    )
    secret_setting = result.scalar_one_or_none()

    if not secret_setting:
        logger.warning("oauth2-proxy client exists but no stored secret found")
        return False

    # Sync to Kubernetes
    return sync_oauth2_credentials_to_k8s(
        client_id="oauth2-proxy",
        client_secret=secret_setting.value
    )
