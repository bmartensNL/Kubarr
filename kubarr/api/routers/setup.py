"""Setup wizard API endpoints."""

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel, EmailStr
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.dependencies import get_db
from kubarr.core.setup import (
    generate_initial_credentials,
    get_setup_status,
    initialize_setup,
    is_setup_required,
)

router = APIRouter()


async def verify_setup_required(db: AsyncSession = Depends(get_db)):
    """Verify that setup is still required.

    This dependency ensures setup endpoints are only accessible
    when initial setup hasn't been completed yet.

    Args:
        db: Database session

    Raises:
        HTTPException: If setup has already been completed
    """
    if not await is_setup_required(db):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Setup has already been completed. These endpoints are only available during initial setup."
        )
    return db


class SetupRequest(BaseModel):
    """Setup request model."""

    admin_username: str
    admin_email: EmailStr
    admin_password: str
    base_url: str
    oauth2_client_secret: str | None = None


class SetupStatusResponse(BaseModel):
    """Setup status response model."""

    setup_required: bool
    admin_user_exists: bool
    oauth2_client_exists: bool


class GeneratedCredentialsResponse(BaseModel):
    """Generated credentials response model."""

    admin_username: str
    admin_email: str
    admin_password: str
    client_secret: str


@router.get("/status", response_model=SetupStatusResponse)
async def setup_status(db: AsyncSession = Depends(verify_setup_required)):
    """Check setup status (only accessible during initial setup).

    Args:
        db: Database session

    Returns:
        Setup status
    """
    status_data = await get_setup_status(db)
    return SetupStatusResponse(**status_data)


@router.post("/initialize")
async def initialize(setup_data: SetupRequest, db: AsyncSession = Depends(verify_setup_required)):
    """Initialize the dashboard with admin user and OAuth2 client (only accessible during initial setup).

    Args:
        setup_data: Setup request data
        db: Database session

    Returns:
        Setup results

    Raises:
        HTTPException: If setup has already been completed
    """
    try:
        result = await initialize_setup(
            db=db,
            admin_username=setup_data.admin_username,
            admin_email=setup_data.admin_email,
            admin_password=setup_data.admin_password,
            base_url=setup_data.base_url,
            oauth2_client_secret=setup_data.oauth2_client_secret,
        )

        return {
            "success": True,
            "message": "Setup completed successfully",
            "data": result,
        }
    except ValueError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail=str(e)
        )


@router.get("/generate-credentials", response_model=GeneratedCredentialsResponse, dependencies=[Depends(verify_setup_required)])
async def generate_credentials():
    """Generate random credentials for initial setup (only accessible during initial setup).

    This endpoint can be used to generate secure random credentials
    for the admin user and OAuth2 client. It is only accessible when
    setup has not yet been completed.

    Returns:
        Generated credentials
    """
    credentials = generate_initial_credentials()
    return GeneratedCredentialsResponse(**credentials)


@router.get("/required")
async def check_setup_required(db: AsyncSession = Depends(get_db)):
    """Simple endpoint to check if setup is required.

    Args:
        db: Database session

    Returns:
        Boolean indicating if setup is required
    """
    required = await is_setup_required(db)
    return {"setup_required": required}
