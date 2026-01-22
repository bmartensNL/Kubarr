"""SQLAlchemy models for authentication and OAuth2."""

from datetime import datetime
from typing import Optional

from sqlalchemy import Boolean, Column, DateTime, ForeignKey, Integer, String, Text
from sqlalchemy.orm import relationship

from kubarr.core.database import Base


class User(Base):
    """User model for authentication."""

    __tablename__ = "users"

    id = Column(Integer, primary_key=True, index=True)
    username = Column(String(50), unique=True, nullable=False, index=True)
    email = Column(String(255), unique=True, nullable=False, index=True)
    hashed_password = Column(String(255), nullable=False)
    is_active = Column(Boolean, default=True, nullable=False)
    is_admin = Column(Boolean, default=False, nullable=False)
    is_approved = Column(Boolean, default=False, nullable=False)
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)
    updated_at = Column(DateTime, default=datetime.utcnow, onupdate=datetime.utcnow, nullable=False)

    # Relationships
    tokens = relationship("OAuth2Token", back_populates="user", cascade="all, delete-orphan")
    auth_codes = relationship("OAuth2AuthorizationCode", back_populates="user", cascade="all, delete-orphan")

    def __repr__(self) -> str:
        return f"<User(id={self.id}, username='{self.username}', email='{self.email}')>"


class OAuth2Client(Base):
    """OAuth2 client model."""

    __tablename__ = "oauth2_clients"

    client_id = Column(String(255), primary_key=True, index=True)
    client_secret_hash = Column(String(255), nullable=False)
    name = Column(String(255), nullable=False)
    redirect_uris = Column(Text, nullable=False)  # JSON array as text
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)

    # Relationships
    tokens = relationship("OAuth2Token", back_populates="client", cascade="all, delete-orphan")
    auth_codes = relationship("OAuth2AuthorizationCode", back_populates="client", cascade="all, delete-orphan")

    def __repr__(self) -> str:
        return f"<OAuth2Client(client_id='{self.client_id}', name='{self.name}')>"


class OAuth2AuthorizationCode(Base):
    """OAuth2 authorization code model."""

    __tablename__ = "oauth2_authorization_codes"

    code = Column(String(255), primary_key=True, index=True)
    client_id = Column(String(255), ForeignKey("oauth2_clients.client_id"), nullable=False, index=True)
    user_id = Column(Integer, ForeignKey("users.id"), nullable=False, index=True)
    redirect_uri = Column(Text, nullable=False)
    scope = Column(Text, nullable=True)
    code_challenge = Column(String(255), nullable=True)
    code_challenge_method = Column(String(10), nullable=True)
    expires_at = Column(DateTime, nullable=False, index=True)
    used = Column(Boolean, default=False, nullable=False)
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)

    # Relationships
    client = relationship("OAuth2Client", back_populates="auth_codes")
    user = relationship("User", back_populates="auth_codes")

    def __repr__(self) -> str:
        return f"<OAuth2AuthorizationCode(code='{self.code[:10]}...', client_id='{self.client_id}')>"


class OAuth2Token(Base):
    """OAuth2 token model."""

    __tablename__ = "oauth2_tokens"

    id = Column(Integer, primary_key=True, index=True)
    access_token = Column(String(500), unique=True, nullable=False, index=True)
    refresh_token = Column(String(500), unique=True, nullable=True, index=True)
    client_id = Column(String(255), ForeignKey("oauth2_clients.client_id"), nullable=False, index=True)
    user_id = Column(Integer, ForeignKey("users.id"), nullable=False, index=True)
    scope = Column(Text, nullable=True)
    expires_at = Column(DateTime, nullable=False, index=True)
    refresh_expires_at = Column(DateTime, nullable=True, index=True)
    revoked = Column(Boolean, default=False, nullable=False)
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)

    # Relationships
    client = relationship("OAuth2Client", back_populates="tokens")
    user = relationship("User", back_populates="tokens")

    def __repr__(self) -> str:
        return f"<OAuth2Token(id={self.id}, client_id='{self.client_id}', user_id={self.user_id})>"
