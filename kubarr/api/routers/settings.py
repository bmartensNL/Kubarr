"""System settings API endpoints."""

from typing import Dict, List, Optional

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.dependencies import get_db, get_current_admin_user
from kubarr.core.models_auth import SystemSettings, User

router = APIRouter(tags=["settings"])

# Default settings values
DEFAULT_SETTINGS = {
    "registration_enabled": {
        "value": "true",
        "description": "Allow new user registration (invites still work when disabled)",
    },
    "registration_require_approval": {
        "value": "true",
        "description": "Require admin approval for new registrations",
    },
}


class SettingResponse(BaseModel):
    """Setting response model."""

    key: str
    value: str
    description: Optional[str] = None


class SettingUpdate(BaseModel):
    """Setting update model."""

    value: str


class SettingsResponse(BaseModel):
    """All settings response model."""

    settings: Dict[str, SettingResponse]


async def get_setting(db: AsyncSession, key: str) -> Optional[str]:
    """Get a setting value from the database."""
    result = await db.execute(select(SystemSettings).where(SystemSettings.key == key))
    setting = result.scalar_one_or_none()
    if setting:
        return setting.value
    # Return default if exists
    if key in DEFAULT_SETTINGS:
        return DEFAULT_SETTINGS[key]["value"]
    return None


async def get_setting_bool(db: AsyncSession, key: str) -> bool:
    """Get a boolean setting value."""
    value = await get_setting(db, key)
    return value.lower() in ("true", "1", "yes") if value else False


@router.get("/", response_model=SettingsResponse)
async def list_settings(
    db: AsyncSession = Depends(get_db),
    current_user: User = Depends(get_current_admin_user),
):
    """List all system settings. Admin only."""
    # Get all settings from database
    result = await db.execute(select(SystemSettings))
    db_settings = {s.key: s for s in result.scalars().all()}

    # Merge with defaults
    settings = {}
    for key, default in DEFAULT_SETTINGS.items():
        if key in db_settings:
            settings[key] = SettingResponse(
                key=key,
                value=db_settings[key].value,
                description=db_settings[key].description or default["description"],
            )
        else:
            settings[key] = SettingResponse(
                key=key,
                value=default["value"],
                description=default["description"],
            )

    return SettingsResponse(settings=settings)


@router.get("/{key}", response_model=SettingResponse)
async def get_setting_endpoint(
    key: str,
    db: AsyncSession = Depends(get_db),
    current_user: User = Depends(get_current_admin_user),
):
    """Get a specific setting. Admin only."""
    result = await db.execute(select(SystemSettings).where(SystemSettings.key == key))
    setting = result.scalar_one_or_none()

    if setting:
        return SettingResponse(
            key=setting.key,
            value=setting.value,
            description=setting.description,
        )

    # Check defaults
    if key in DEFAULT_SETTINGS:
        return SettingResponse(
            key=key,
            value=DEFAULT_SETTINGS[key]["value"],
            description=DEFAULT_SETTINGS[key]["description"],
        )

    raise HTTPException(
        status_code=status.HTTP_404_NOT_FOUND,
        detail=f"Setting '{key}' not found",
    )


@router.put("/{key}", response_model=SettingResponse)
async def update_setting(
    key: str,
    update: SettingUpdate,
    db: AsyncSession = Depends(get_db),
    current_user: User = Depends(get_current_admin_user),
):
    """Update a system setting. Admin only."""
    # Validate key exists in defaults
    if key not in DEFAULT_SETTINGS:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Unknown setting key '{key}'",
        )

    result = await db.execute(select(SystemSettings).where(SystemSettings.key == key))
    setting = result.scalar_one_or_none()

    if setting:
        setting.value = update.value
    else:
        setting = SystemSettings(
            key=key,
            value=update.value,
            description=DEFAULT_SETTINGS[key]["description"],
        )
        db.add(setting)

    await db.commit()
    await db.refresh(setting)

    return SettingResponse(
        key=setting.key,
        value=setting.value,
        description=setting.description,
    )
