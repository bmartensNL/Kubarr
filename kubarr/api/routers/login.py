"""Login and registration endpoints with HTML templates."""

from datetime import datetime
from typing import Optional

from fastapi import APIRouter, Depends, Form, HTTPException, Request, status
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates
from pydantic import EmailStr
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.config import settings
from kubarr.api.dependencies import get_db
from kubarr.core.models_auth import Invite, SystemSettings, User
from kubarr.core.oauth2_service import OAuth2Service
from kubarr.core.security import hash_password, verify_password


async def get_registration_enabled(db: AsyncSession) -> bool:
    """Check if registration is enabled (from DB or config fallback)."""
    result = await db.execute(
        select(SystemSettings).where(SystemSettings.key == "registration_enabled")
    )
    setting = result.scalar_one_or_none()
    if setting:
        return setting.value.lower() in ("true", "1", "yes")
    return settings.registration_enabled


async def get_registration_require_approval(db: AsyncSession) -> bool:
    """Check if registration requires approval (from DB or config fallback)."""
    result = await db.execute(
        select(SystemSettings).where(SystemSettings.key == "registration_require_approval")
    )
    setting = result.scalar_one_or_none()
    if setting:
        return setting.value.lower() in ("true", "1", "yes")
    return settings.registration_require_approval

router = APIRouter()

# Jinja2 templates
templates = Jinja2Templates(directory="kubarr/api/templates")


@router.get("/login", response_class=HTMLResponse)
async def login_page(
    request: Request,
    client_id: Optional[str] = None,
    redirect_uri: Optional[str] = None,
    scope: Optional[str] = None,
    state: Optional[str] = None,
    code_challenge: Optional[str] = None,
    code_challenge_method: Optional[str] = "S256",
    error: Optional[str] = None,
):
    """Render login page.

    Args:
        request: FastAPI request
        client_id: OAuth2 client ID
        redirect_uri: Redirect URI after login
        scope: Requested scope
        state: State parameter
        code_challenge: PKCE code challenge
        code_challenge_method: PKCE method
        error: Error message to display

    Returns:
        HTML login page
    """
    return templates.TemplateResponse(
        "login.html",
        {
            "request": request,
            "client_id": client_id or "",
            "redirect_uri": redirect_uri or "",
            "scope": scope or "",
            "state": state or "",
            "code_challenge": code_challenge or "",
            "code_challenge_method": code_challenge_method,
            "error": error,
        },
    )


@router.post("/login")
async def login_submit(
    username: str = Form(...),
    password: str = Form(...),
    client_id: str = Form(...),
    redirect_uri: str = Form(...),
    scope: Optional[str] = Form(None),
    state: Optional[str] = Form(None),
    code_challenge: Optional[str] = Form(None),
    code_challenge_method: Optional[str] = Form("S256"),
    db: AsyncSession = Depends(get_db),
):
    """Handle login form submission.

    Args:
        username: Username
        password: Password
        client_id: OAuth2 client ID
        redirect_uri: Redirect URI
        scope: Requested scope
        state: State parameter
        code_challenge: PKCE code challenge
        code_challenge_method: PKCE method
        db: Database session

    Returns:
        Redirect to callback with authorization code or error
    """
    # Validate user credentials
    result = await db.execute(select(User).where(User.username == username))
    user = result.scalar_one_or_none()

    if not user or not verify_password(password, user.hashed_password):
        # Redirect back to login with error
        return RedirectResponse(
            url=f"/auth/login?client_id={client_id}&redirect_uri={redirect_uri}"
            f"&scope={scope or ''}&state={state or ''}"
            f"&code_challenge={code_challenge or ''}"
            f"&code_challenge_method={code_challenge_method}"
            f"&error=Invalid credentials",
            status_code=status.HTTP_302_FOUND,
        )

    # Check if user is active
    if not user.is_active:
        return RedirectResponse(
            url=f"/auth/login?client_id={client_id}&redirect_uri={redirect_uri}"
            f"&error=Account is inactive",
            status_code=status.HTTP_302_FOUND,
        )

    # Check if user is approved
    if not user.is_approved:
        return RedirectResponse(
            url=f"/auth/login?client_id={client_id}&redirect_uri={redirect_uri}"
            f"&error=Account pending approval",
            status_code=status.HTTP_302_FOUND,
        )

    # Create authorization code
    oauth2_service = OAuth2Service(db)
    auth_code = await oauth2_service.create_authorization_code(
        client_id=client_id,
        user_id=user.id,
        redirect_uri=redirect_uri,
        scope=scope,
        code_challenge=code_challenge,
        code_challenge_method=code_challenge_method,
    )

    # Build callback URL
    callback_url = f"{redirect_uri}?code={auth_code}"
    if state:
        callback_url += f"&state={state}"

    return RedirectResponse(url=callback_url, status_code=status.HTTP_302_FOUND)


@router.get("/register", response_class=HTMLResponse)
async def register_page(
    request: Request,
    error: Optional[str] = None,
    success: Optional[bool] = None,
    invite: Optional[str] = None,
    db: AsyncSession = Depends(get_db),
):
    """Render registration page.

    Args:
        request: FastAPI request
        error: Error message to display
        success: Success flag
        invite: Invite code from URL
        db: Database session

    Returns:
        HTML registration page
    """
    # Check if registration is disabled (no open registration)
    # If invite code is provided, allow registration even if open registration is disabled
    invite_valid = False
    registration_enabled = await get_registration_enabled(db)
    invite_required = not registration_enabled

    if invite:
        # Validate invite code
        result = await db.execute(
            select(Invite).where(Invite.code == invite, Invite.is_used == False)
        )
        invite_obj = result.scalar_one_or_none()
        if invite_obj:
            # Check expiration
            if invite_obj.expires_at is None or invite_obj.expires_at > datetime.utcnow():
                invite_valid = True

    # If registration is disabled and no valid invite, show error
    if not registration_enabled and not invite_valid:
        return templates.TemplateResponse(
            "register.html",
            {
                "request": request,
                "error": "Registration requires a valid invite link" if invite else "Registration is disabled. Please request an invite link from an administrator.",
                "success": False,
                "require_approval": False,
                "invite_code": invite or "",
                "invite_required": True,
            },
        )

    require_approval = await get_registration_require_approval(db)
    return templates.TemplateResponse(
        "register.html",
        {
            "request": request,
            "error": error,
            "success": success,
            "require_approval": require_approval and not invite_valid,
            "invite_code": invite or "",
            "invite_required": invite_required,
        },
    )


@router.post("/register")
async def register_submit(
    username: str = Form(...),
    email: EmailStr = Form(...),
    password: str = Form(...),
    confirm_password: str = Form(...),
    invite_code: Optional[str] = Form(None),
    db: AsyncSession = Depends(get_db),
):
    """Handle registration form submission.

    Args:
        username: Username
        email: Email address
        password: Password
        confirm_password: Password confirmation
        invite_code: Optional invite code
        db: Database session

    Returns:
        Redirect to registration page with success/error
    """
    # Validate invite code if provided or required
    invite_obj = None
    invite_valid = False
    invite_url_param = f"&invite={invite_code}" if invite_code else ""

    if invite_code:
        result = await db.execute(
            select(Invite).where(Invite.code == invite_code, Invite.is_used == False)
        )
        invite_obj = result.scalar_one_or_none()
        if invite_obj:
            # Check expiration
            if invite_obj.expires_at is None or invite_obj.expires_at > datetime.utcnow():
                invite_valid = True

    # If registration is disabled and no valid invite, reject
    registration_enabled = await get_registration_enabled(db)
    if not registration_enabled and not invite_valid:
        return RedirectResponse(
            url=f"/auth/register?error=Registration requires a valid invite link{invite_url_param}",
            status_code=status.HTTP_302_FOUND,
        )

    # Validate password match
    if password != confirm_password:
        return RedirectResponse(
            url=f"/auth/register?error=Passwords do not match{invite_url_param}",
            status_code=status.HTTP_302_FOUND,
        )

    # Validate password length
    if len(password) < 8:
        return RedirectResponse(
            url=f"/auth/register?error=Password must be at least 8 characters{invite_url_param}",
            status_code=status.HTTP_302_FOUND,
        )

    # Check if username exists
    result = await db.execute(select(User).where(User.username == username))
    if result.scalar_one_or_none():
        return RedirectResponse(
            url=f"/auth/register?error=Username already exists{invite_url_param}",
            status_code=status.HTTP_302_FOUND,
        )

    # Check if email exists
    result = await db.execute(select(User).where(User.email == email))
    if result.scalar_one_or_none():
        return RedirectResponse(
            url=f"/auth/register?error=Email already exists{invite_url_param}",
            status_code=status.HTTP_302_FOUND,
        )

    # Create user (auto-approve if using valid invite)
    require_approval = await get_registration_require_approval(db)
    new_user = User(
        username=username,
        email=email,
        hashed_password=hash_password(password),
        is_admin=False,
        is_active=True,
        is_approved=invite_valid or not require_approval,
    )

    db.add(new_user)
    await db.flush()  # Get the user ID

    # Mark invite as used if valid
    if invite_valid and invite_obj:
        invite_obj.is_used = True
        invite_obj.used_by_id = new_user.id
        invite_obj.used_at = datetime.utcnow()

    await db.commit()

    # Redirect with success message
    return RedirectResponse(
        url="/auth/register?success=true", status_code=status.HTTP_302_FOUND
    )


@router.post("/logout")
async def logout(
    token: Optional[str] = Form(None), db: AsyncSession = Depends(get_db)
):
    """Handle logout.

    Args:
        token: Access token to revoke
        db: Database session

    Returns:
        Success message
    """
    if token:
        oauth2_service = OAuth2Service(db)
        await oauth2_service.revoke_token(token)

    return {"message": "Logged out successfully"}
