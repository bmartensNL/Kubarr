"""Security utilities for password hashing, JWT tokens, and cryptography."""

import hashlib
import os
import secrets
from datetime import datetime, timedelta
from typing import Any, Dict, Optional

from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from jose import JWTError, jwt
from passlib.context import CryptContext

# Password hashing context
pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")

# JWT settings
JWT_ALGORITHM = os.getenv("KUBARR_JWT_ALGORITHM", "RS256")
JWT_ACCESS_TOKEN_EXPIRE = int(os.getenv("KUBARR_JWT_ACCESS_TOKEN_EXPIRE", "3600"))
JWT_REFRESH_TOKEN_EXPIRE = int(os.getenv("KUBARR_JWT_REFRESH_TOKEN_EXPIRE", "604800"))

# JWT key paths
JWT_PRIVATE_KEY_PATH = os.getenv("KUBARR_JWT_PRIVATE_KEY_PATH", "/secrets/jwt-private.pem")
JWT_PUBLIC_KEY_PATH = os.getenv("KUBARR_JWT_PUBLIC_KEY_PATH", "/secrets/jwt-public.pem")

# Fallback to in-memory keys if files don't exist (for development)
_private_key: Optional[str] = None
_public_key: Optional[str] = None


def get_private_key() -> str:
    """Get JWT private key.

    Returns:
        str: Private key in PEM format
    """
    global _private_key

    if _private_key:
        return _private_key

    if os.path.exists(JWT_PRIVATE_KEY_PATH):
        with open(JWT_PRIVATE_KEY_PATH, "r") as f:
            _private_key = f.read()
            return _private_key

    # Generate in-memory key for development
    print("WARNING: JWT private key not found, generating temporary key")
    _private_key, _public_key = generate_rsa_key_pair()
    return _private_key


def get_public_key() -> str:
    """Get JWT public key.

    Returns:
        str: Public key in PEM format
    """
    global _public_key

    if _public_key:
        return _public_key

    if os.path.exists(JWT_PUBLIC_KEY_PATH):
        with open(JWT_PUBLIC_KEY_PATH, "r") as f:
            _public_key = f.read()
            return _public_key

    # Use public key from generated pair
    if _private_key:
        return _public_key

    # Trigger private key generation which also generates public key
    get_private_key()
    return _public_key


def generate_rsa_key_pair() -> tuple[str, str]:
    """Generate RSA key pair for JWT signing.

    Returns:
        tuple: (private_key_pem, public_key_pem)
    """
    # Generate private key
    private_key = rsa.generate_private_key(
        public_exponent=65537,
        key_size=2048,
        backend=default_backend()
    )

    # Serialize private key
    private_pem = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    ).decode("utf-8")

    # Serialize public key
    public_key = private_key.public_key()
    public_pem = public_key.public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo
    ).decode("utf-8")

    return private_pem, public_pem


def hash_password(password: str) -> str:
    """Hash a password using bcrypt.

    Args:
        password: Plain text password

    Returns:
        str: Hashed password
    """
    return pwd_context.hash(password)


def verify_password(plain_password: str, hashed_password: str) -> bool:
    """Verify a password against its hash.

    Args:
        plain_password: Plain text password
        hashed_password: Hashed password

    Returns:
        bool: True if password matches
    """
    return pwd_context.verify(plain_password, hashed_password)


def create_access_token(
    data: Dict[str, Any],
    expires_delta: Optional[timedelta] = None
) -> str:
    """Create a JWT access token.

    Args:
        data: Data to encode in the token
        expires_delta: Custom expiration time

    Returns:
        str: JWT token
    """
    to_encode = data.copy()

    if expires_delta:
        expire = datetime.utcnow() + expires_delta
    else:
        expire = datetime.utcnow() + timedelta(seconds=JWT_ACCESS_TOKEN_EXPIRE)

    to_encode.update({
        "exp": expire,
        "iat": datetime.utcnow()
    })

    private_key = get_private_key()
    encoded_jwt = jwt.encode(to_encode, private_key, algorithm=JWT_ALGORITHM)
    return encoded_jwt


def create_refresh_token(
    data: Dict[str, Any],
    expires_delta: Optional[timedelta] = None
) -> str:
    """Create a JWT refresh token.

    Args:
        data: Data to encode in the token
        expires_delta: Custom expiration time

    Returns:
        str: JWT refresh token
    """
    to_encode = data.copy()

    if expires_delta:
        expire = datetime.utcnow() + expires_delta
    else:
        expire = datetime.utcnow() + timedelta(seconds=JWT_REFRESH_TOKEN_EXPIRE)

    to_encode.update({
        "exp": expire,
        "iat": datetime.utcnow(),
        "type": "refresh"
    })

    private_key = get_private_key()
    encoded_jwt = jwt.encode(to_encode, private_key, algorithm=JWT_ALGORITHM)
    return encoded_jwt


def decode_token(token: str) -> Dict[str, Any]:
    """Decode and validate a JWT token.

    Args:
        token: JWT token

    Returns:
        dict: Decoded token payload

    Raises:
        JWTError: If token is invalid
    """
    public_key = get_public_key()
    payload = jwt.decode(token, public_key, algorithms=[JWT_ALGORITHM])
    return payload


def generate_random_string(length: int = 32) -> str:
    """Generate a cryptographically secure random string.

    Args:
        length: Length of the string

    Returns:
        str: Random hex string
    """
    return secrets.token_hex(length)


def generate_authorization_code() -> str:
    """Generate an OAuth2 authorization code.

    Returns:
        str: Authorization code
    """
    return generate_random_string(32)


def hash_client_secret(secret: str) -> str:
    """Hash an OAuth2 client secret.

    Args:
        secret: Plain text client secret

    Returns:
        str: Hashed secret
    """
    return hash_password(secret)


def verify_client_secret(plain_secret: str, hashed_secret: str) -> bool:
    """Verify an OAuth2 client secret.

    Args:
        plain_secret: Plain text secret
        hashed_secret: Hashed secret

    Returns:
        bool: True if secret matches
    """
    return verify_password(plain_secret, hashed_secret)


def verify_pkce(code_verifier: str, code_challenge: str, method: str = "S256") -> bool:
    """Verify PKCE code challenge.

    Args:
        code_verifier: Code verifier from token request
        code_challenge: Code challenge from authorization request
        method: Challenge method (S256 or plain)

    Returns:
        bool: True if verification passes
    """
    if method == "S256":
        # SHA256 hash and base64url encode
        digest = hashlib.sha256(code_verifier.encode()).digest()
        computed_challenge = (
            digest.hex()
        )
        return computed_challenge == code_challenge
    elif method == "plain":
        return code_verifier == code_challenge
    else:
        return False


def generate_cookie_secret() -> str:
    """Generate a secure cookie secret for oauth2-proxy.

    Returns:
        str: Base64-encoded 32-byte random string
    """
    import base64
    random_bytes = secrets.token_bytes(32)
    return base64.b64encode(random_bytes).decode("utf-8")


def generate_secure_password(length: int = 16) -> str:
    """Generate a secure random password.

    Args:
        length: Password length

    Returns:
        str: Random password
    """
    import string
    alphabet = string.ascii_letters + string.digits + "!@#$%^&*()"
    password = ''.join(secrets.choice(alphabet) for _ in range(length))
    return password
