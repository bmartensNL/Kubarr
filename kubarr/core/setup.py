"""Setup wizard for initial Kubarr configuration."""

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.config import settings
from kubarr.core.models_auth import OAuth2Client, User
from kubarr.core.security import (
    generate_random_string,
    generate_secure_password,
    hash_client_secret,
    hash_password,
)


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
    base_url: str,
    oauth2_client_secret: str | None = None,
) -> dict:
    """Run the complete initial setup.

    Args:
        db: Database session
        admin_username: Admin username
        admin_email: Admin email
        admin_password: Admin password
        base_url: Base URL for the application
        oauth2_client_secret: OAuth2 client secret (generated if not provided)

    Returns:
        Dictionary with setup results

    Raises:
        ValueError: If setup has already been completed
    """
    # Check if setup is required
    if not await is_setup_required(db):
        raise ValueError("Setup has already been completed")

    # Create admin user
    admin_user = await create_admin_user(db, admin_username, admin_email, admin_password)

    # Create oauth2-proxy client
    redirect_uris = [
        f"{base_url}/oauth2/callback",
        f"{base_url}/oauth/callback",  # Alternative
    ]

    client, plain_secret = await create_oauth2_proxy_client(
        db, redirect_uris, oauth2_client_secret
    )

    return {
        "admin_user": {
            "id": admin_user.id,
            "username": admin_user.username,
            "email": admin_user.email,
        },
        "oauth2_client": {
            "client_id": client.client_id,
            "client_secret": plain_secret,
            "redirect_uris": redirect_uris,
        },
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

    return {
        "setup_required": admin_user is None,
        "admin_user_exists": admin_user is not None,
        "oauth2_client_exists": oauth2_client is not None,
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
