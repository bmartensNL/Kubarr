"""OAuth2 authorization server service."""

import json
from datetime import datetime, timedelta
from typing import Dict, Optional, Tuple

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.core.models_auth import (
    OAuth2AuthorizationCode,
    OAuth2Client,
    OAuth2Token,
    User,
)
from kubarr.core.security import (
    create_access_token,
    create_refresh_token,
    generate_authorization_code,
    verify_client_secret,
    verify_pkce,
)


class OAuth2Service:
    """Service for handling OAuth2 operations."""

    def __init__(self, db: AsyncSession):
        """Initialize OAuth2 service.

        Args:
            db: Database session
        """
        self.db = db

    async def get_client(self, client_id: str) -> Optional[OAuth2Client]:
        """Get OAuth2 client by ID.

        Args:
            client_id: Client ID

        Returns:
            OAuth2Client or None
        """
        result = await self.db.execute(
            select(OAuth2Client).where(OAuth2Client.client_id == client_id)
        )
        return result.scalar_one_or_none()

    async def validate_client(
        self, client_id: str, client_secret: Optional[str] = None
    ) -> bool:
        """Validate OAuth2 client credentials.

        Args:
            client_id: Client ID
            client_secret: Client secret (optional for public clients)

        Returns:
            True if valid
        """
        client = await self.get_client(client_id)
        if not client:
            return False

        if client_secret:
            return verify_client_secret(client_secret, client.client_secret_hash)

        return True

    async def create_authorization_code(
        self,
        client_id: str,
        user_id: int,
        redirect_uri: str,
        scope: Optional[str] = None,
        code_challenge: Optional[str] = None,
        code_challenge_method: Optional[str] = None,
        expires_in: int = 600,  # 10 minutes
    ) -> str:
        """Create an authorization code.

        Args:
            client_id: Client ID
            user_id: User ID
            redirect_uri: Redirect URI
            scope: Requested scope
            code_challenge: PKCE code challenge
            code_challenge_method: PKCE method (S256 or plain)
            expires_in: Code expiration in seconds

        Returns:
            Authorization code
        """
        code = generate_authorization_code()
        expires_at = datetime.utcnow() + timedelta(seconds=expires_in)

        auth_code = OAuth2AuthorizationCode(
            code=code,
            client_id=client_id,
            user_id=user_id,
            redirect_uri=redirect_uri,
            scope=scope,
            code_challenge=code_challenge,
            code_challenge_method=code_challenge_method,
            expires_at=expires_at,
            used=False,
        )

        self.db.add(auth_code)
        await self.db.commit()

        return code

    async def validate_authorization_code(
        self,
        code: str,
        client_id: str,
        redirect_uri: str,
        code_verifier: Optional[str] = None,
    ) -> Optional[OAuth2AuthorizationCode]:
        """Validate and consume an authorization code.

        Args:
            code: Authorization code
            client_id: Client ID
            redirect_uri: Redirect URI
            code_verifier: PKCE code verifier

        Returns:
            OAuth2AuthorizationCode or None if invalid
        """
        result = await self.db.execute(
            select(OAuth2AuthorizationCode).where(
                OAuth2AuthorizationCode.code == code
            )
        )
        auth_code = result.scalar_one_or_none()

        if not auth_code:
            return None

        # Check if already used
        if auth_code.used:
            return None

        # Check expiration
        if datetime.utcnow() > auth_code.expires_at:
            return None

        # Check client ID
        if auth_code.client_id != client_id:
            return None

        # Check redirect URI
        if auth_code.redirect_uri != redirect_uri:
            return None

        # Verify PKCE if present
        if auth_code.code_challenge:
            if not code_verifier:
                return None
            if not verify_pkce(
                code_verifier,
                auth_code.code_challenge,
                auth_code.code_challenge_method or "S256",
            ):
                return None

        # Mark as used
        auth_code.used = True
        await self.db.commit()

        return auth_code

    async def create_tokens(
        self,
        client_id: str,
        user_id: int,
        scope: Optional[str] = None,
        access_token_expires_in: int = 3600,  # 1 hour
        refresh_token_expires_in: int = 604800,  # 7 days
    ) -> Tuple[str, str, int, int]:
        """Create access and refresh tokens.

        Args:
            client_id: Client ID
            user_id: User ID
            scope: Token scope
            access_token_expires_in: Access token expiration in seconds
            refresh_token_expires_in: Refresh token expiration in seconds

        Returns:
            Tuple of (access_token, refresh_token, expires_in, refresh_expires_in)
        """
        # Get user info
        result = await self.db.execute(select(User).where(User.id == user_id))
        user = result.scalar_one_or_none()

        if not user:
            raise ValueError("User not found")

        # Create access token
        access_token_data = {
            "sub": str(user.id),
            "username": user.username,
            "email": user.email,
            "aud": [client_id],
            "scope": scope or "openid profile email",
        }
        access_token = create_access_token(
            access_token_data,
            expires_delta=timedelta(seconds=access_token_expires_in),
        )

        # Create refresh token
        refresh_token_data = {
            "sub": str(user.id),
            "type": "refresh",
        }
        refresh_token = create_refresh_token(
            refresh_token_data,
            expires_delta=timedelta(seconds=refresh_token_expires_in),
        )

        # Store in database
        expires_at = datetime.utcnow() + timedelta(seconds=access_token_expires_in)
        refresh_expires_at = datetime.utcnow() + timedelta(
            seconds=refresh_token_expires_in
        )

        token_record = OAuth2Token(
            access_token=access_token,
            refresh_token=refresh_token,
            client_id=client_id,
            user_id=user_id,
            scope=scope,
            expires_at=expires_at,
            refresh_expires_at=refresh_expires_at,
            revoked=False,
        )

        self.db.add(token_record)
        await self.db.commit()

        return (
            access_token,
            refresh_token,
            access_token_expires_in,
            refresh_token_expires_in,
        )

    async def validate_access_token(self, access_token: str) -> Optional[OAuth2Token]:
        """Validate an access token.

        Args:
            access_token: Access token

        Returns:
            OAuth2Token or None if invalid
        """
        result = await self.db.execute(
            select(OAuth2Token).where(OAuth2Token.access_token == access_token)
        )
        token = result.scalar_one_or_none()

        if not token:
            return None

        # Check if revoked
        if token.revoked:
            return None

        # Check expiration
        if datetime.utcnow() > token.expires_at:
            return None

        return token

    async def refresh_access_token(
        self, refresh_token: str, client_id: str
    ) -> Optional[Tuple[str, str, int, int]]:
        """Refresh an access token.

        Args:
            refresh_token: Refresh token
            client_id: Client ID

        Returns:
            Tuple of (new_access_token, new_refresh_token, expires_in, refresh_expires_in) or None
        """
        result = await self.db.execute(
            select(OAuth2Token).where(OAuth2Token.refresh_token == refresh_token)
        )
        token = result.scalar_one_or_none()

        if not token:
            return None

        # Check client ID
        if token.client_id != client_id:
            return None

        # Check if revoked
        if token.revoked:
            return None

        # Check refresh token expiration
        if (
            token.refresh_expires_at
            and datetime.utcnow() > token.refresh_expires_at
        ):
            return None

        # Revoke old tokens
        token.revoked = True
        await self.db.commit()

        # Create new tokens
        return await self.create_tokens(
            client_id=client_id, user_id=token.user_id, scope=token.scope
        )

    async def revoke_token(self, token: str) -> bool:
        """Revoke an access or refresh token.

        Args:
            token: Token to revoke

        Returns:
            True if revoked
        """
        # Try as access token
        result = await self.db.execute(
            select(OAuth2Token).where(OAuth2Token.access_token == token)
        )
        token_record = result.scalar_one_or_none()

        # Try as refresh token if not found
        if not token_record:
            result = await self.db.execute(
                select(OAuth2Token).where(OAuth2Token.refresh_token == token)
            )
            token_record = result.scalar_one_or_none()

        if not token_record:
            return False

        token_record.revoked = True
        await self.db.commit()

        return True

    async def introspect_token(self, token: str) -> Optional[Dict]:
        """Introspect a token (for oauth2-proxy).

        Args:
            token: Token to introspect

        Returns:
            Token information or None
        """
        token_record = await self.validate_access_token(token)

        if not token_record:
            return {"active": False}

        # Get user
        result = await self.db.execute(
            select(User).where(User.id == token_record.user_id)
        )
        user = result.scalar_one_or_none()

        if not user or not user.is_active or not user.is_approved:
            return {"active": False}

        return {
            "active": True,
            "sub": str(user.id),
            "username": user.username,
            "email": user.email,
            "scope": token_record.scope,
            "exp": int(token_record.expires_at.timestamp()),
            "client_id": token_record.client_id,
        }

    async def create_client(
        self, client_id: str, client_secret: str, name: str, redirect_uris: list
    ) -> OAuth2Client:
        """Create an OAuth2 client.

        Args:
            client_id: Client ID
            client_secret: Client secret
            name: Client name
            redirect_uris: List of redirect URIs

        Returns:
            Created OAuth2Client
        """
        from kubarr.core.security import hash_client_secret

        client = OAuth2Client(
            client_id=client_id,
            client_secret_hash=hash_client_secret(client_secret),
            name=name,
            redirect_uris=json.dumps(redirect_uris),
        )

        self.db.add(client)
        await self.db.commit()
        await self.db.refresh(client)

        return client
