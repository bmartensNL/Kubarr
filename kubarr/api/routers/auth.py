"""OAuth2 and OIDC authentication endpoints."""

from typing import Optional

from fastapi import APIRouter, Depends, Form, HTTPException, Request, status
from fastapi.responses import JSONResponse, RedirectResponse
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.config import settings
from kubarr.api.dependencies import get_current_user, get_db
from kubarr.core.models_auth import User
from kubarr.core.oauth2_service import OAuth2Service
from kubarr.core.security import decode_token

router = APIRouter()


# Pydantic schemas
class TokenResponse(BaseModel):
    access_token: str
    token_type: str
    expires_in: int
    refresh_token: Optional[str] = None
    id_token: Optional[str] = None
    scope: Optional[str] = None


class TokenIntrospectionRequest(BaseModel):
    token: str
    client_id: str
    client_secret: Optional[str] = None


class TokenRevocationRequest(BaseModel):
    token: str
    client_id: Optional[str] = None
    client_secret: Optional[str] = None


@router.get("/authorize")
async def authorize(
    request: Request,
    response_type: str,
    client_id: str,
    redirect_uri: str,
    scope: Optional[str] = None,
    state: Optional[str] = None,
    code_challenge: Optional[str] = None,
    code_challenge_method: Optional[str] = "S256",
    db: AsyncSession = Depends(get_db),
):
    """OAuth2 authorization endpoint.

    This endpoint handles the OAuth2 authorization request. It validates the
    client and redirects to the login page if the user is not authenticated.

    Args:
        request: FastAPI request
        response_type: Must be "code"
        client_id: OAuth2 client ID
        redirect_uri: Redirect URI after authorization
        scope: Requested scope
        state: State parameter for CSRF protection
        code_challenge: PKCE code challenge
        code_challenge_method: PKCE method (S256 or plain)
        db: Database session

    Returns:
        Redirect to login page or error
    """
    # Validate response_type
    if response_type != "code":
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Only 'code' response_type is supported",
        )

    # Validate client
    oauth2_service = OAuth2Service(db)
    client = await oauth2_service.get_client(client_id)

    if not client:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid client_id"
        )

    # Build login URL with parameters
    login_url = (
        f"/auth/login?"
        f"client_id={client_id}&"
        f"redirect_uri={redirect_uri}&"
        f"scope={scope or ''}&"
        f"state={state or ''}&"
        f"code_challenge={code_challenge or ''}&"
        f"code_challenge_method={code_challenge_method}"
    )

    return RedirectResponse(url=login_url, status_code=status.HTTP_302_FOUND)


@router.post("/token", response_model=TokenResponse)
async def token(
    grant_type: str = Form(...),
    code: Optional[str] = Form(None),
    redirect_uri: Optional[str] = Form(None),
    client_id: str = Form(...),
    client_secret: Optional[str] = Form(None),
    code_verifier: Optional[str] = Form(None),
    refresh_token: Optional[str] = Form(None),
    db: AsyncSession = Depends(get_db),
):
    """OAuth2 token endpoint.

    Exchanges authorization code for access token, or refreshes tokens.

    Args:
        grant_type: "authorization_code" or "refresh_token"
        code: Authorization code (for authorization_code grant)
        redirect_uri: Redirect URI (for authorization_code grant)
        client_id: OAuth2 client ID
        client_secret: OAuth2 client secret
        code_verifier: PKCE code verifier (for authorization_code grant)
        refresh_token: Refresh token (for refresh_token grant)
        db: Database session

    Returns:
        Token response with access_token, refresh_token, etc.

    Raises:
        HTTPException: If validation fails
    """
    oauth2_service = OAuth2Service(db)

    # Validate client
    if not await oauth2_service.validate_client(client_id, client_secret):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid client credentials",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if grant_type == "authorization_code":
        # Validate required parameters
        if not code or not redirect_uri:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="code and redirect_uri are required",
            )

        # Validate authorization code
        auth_code = await oauth2_service.validate_authorization_code(
            code=code,
            client_id=client_id,
            redirect_uri=redirect_uri,
            code_verifier=code_verifier,
        )

        if not auth_code:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Invalid authorization code",
            )

        # Create tokens
        access_token, refresh_tok, expires_in, refresh_expires = (
            await oauth2_service.create_tokens(
                client_id=client_id,
                user_id=auth_code.user_id,
                scope=auth_code.scope,
                access_token_expires_in=settings.jwt_access_token_expire,
                refresh_token_expires_in=settings.jwt_refresh_token_expire,
            )
        )

        # Create ID token (same as access token for now)
        id_token = access_token

        return TokenResponse(
            access_token=access_token,
            token_type="Bearer",
            expires_in=expires_in,
            refresh_token=refresh_tok,
            id_token=id_token,
            scope=auth_code.scope,
        )

    elif grant_type == "refresh_token":
        # Validate required parameters
        if not refresh_token:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="refresh_token is required",
            )

        # Refresh tokens
        result = await oauth2_service.refresh_access_token(
            refresh_token=refresh_token, client_id=client_id
        )

        if not result:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="Invalid refresh token",
            )

        access_token, refresh_tok, expires_in, refresh_expires = result

        return TokenResponse(
            access_token=access_token,
            token_type="Bearer",
            expires_in=expires_in,
            refresh_token=refresh_tok,
        )

    else:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Unsupported grant_type: {grant_type}",
        )


@router.post("/introspect")
async def introspect(
    data: TokenIntrospectionRequest, db: AsyncSession = Depends(get_db)
):
    """OAuth2 token introspection endpoint.

    Used by oauth2-proxy to validate tokens.

    Args:
        data: Introspection request
        db: Database session

    Returns:
        Token information or active: false
    """
    oauth2_service = OAuth2Service(db)

    # Validate client if credentials provided
    if data.client_secret:
        if not await oauth2_service.validate_client(
            data.client_id, data.client_secret
        ):
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid client credentials",
            )

    # Introspect token
    result = await oauth2_service.introspect_token(data.token)
    return result


@router.get("/userinfo")
async def userinfo(
    current_user: Optional[User] = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    """OIDC UserInfo endpoint.

    Returns information about the authenticated user.

    Args:
        current_user: Current authenticated user
        db: Database session

    Returns:
        User information

    Raises:
        HTTPException: If not authenticated
    """
    if not current_user:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Not authenticated",
            headers={"WWW-Authenticate": "Bearer"},
        )

    return {
        "sub": str(current_user.id),
        "name": current_user.username,
        "preferred_username": current_user.username,
        "email": current_user.email,
        "email_verified": current_user.is_approved,
    }


@router.post("/revoke")
async def revoke(
    data: TokenRevocationRequest, db: AsyncSession = Depends(get_db)
):
    """OAuth2 token revocation endpoint.

    Args:
        data: Revocation request
        db: Database session

    Returns:
        Success message
    """
    oauth2_service = OAuth2Service(db)

    # Validate client if credentials provided
    if data.client_id and data.client_secret:
        if not await oauth2_service.validate_client(
            data.client_id, data.client_secret
        ):
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="Invalid client credentials",
            )

    # Revoke token
    await oauth2_service.revoke_token(data.token)

    return {"message": "Token revoked"}


@router.get("/.well-known/openid-configuration")
async def openid_configuration(request: Request):
    """OIDC Discovery endpoint.

    Returns OpenID Connect discovery metadata.

    Args:
        request: FastAPI request

    Returns:
        OIDC configuration
    """
    base_url = settings.oauth2_issuer_url
    issuer = f"{base_url}/auth"

    return {
        "issuer": issuer,
        "authorization_endpoint": f"{issuer}/authorize",
        "token_endpoint": f"{issuer}/token",
        "userinfo_endpoint": f"{issuer}/userinfo",
        "introspection_endpoint": f"{issuer}/introspect",
        "revocation_endpoint": f"{issuer}/revoke",
        "jwks_uri": f"{issuer}/jwks",
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "token_endpoint_auth_methods_supported": [
            "client_secret_post",
            "client_secret_basic",
        ],
        "code_challenge_methods_supported": ["S256", "plain"],
        "scopes_supported": ["openid", "profile", "email"],
    }


@router.get("/jwks")
async def jwks():
    """JSON Web Key Set endpoint.

    Returns public keys for token verification.

    Returns:
        JWKS
    """
    from kubarr.core.security import get_public_key
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.backends import default_backend
    import base64

    # Get public key
    public_key_pem = get_public_key()

    # Load public key
    public_key = serialization.load_pem_public_key(
        public_key_pem.encode(), backend=default_backend()
    )

    # Extract modulus and exponent for RSA
    from cryptography.hazmat.primitives.asymmetric import rsa

    if isinstance(public_key, rsa.RSAPublicKey):
        public_numbers = public_key.public_numbers()
        n = public_numbers.n
        e = public_numbers.e

        # Convert to base64url
        def int_to_base64url(num):
            num_bytes = num.to_bytes((num.bit_length() + 7) // 8, byteorder="big")
            return (
                base64.urlsafe_b64encode(num_bytes).rstrip(b"=").decode("utf-8")
            )

        return {
            "keys": [
                {
                    "kty": "RSA",
                    "use": "sig",
                    "kid": "kubarr-key-1",
                    "alg": "RS256",
                    "n": int_to_base64url(n),
                    "e": int_to_base64url(e),
                }
            ]
        }

    return {"keys": []}
