"""SQLAlchemy models for authentication and OAuth2."""

from datetime import datetime
from typing import Optional, List, TYPE_CHECKING

from sqlalchemy import Boolean, Column, DateTime, ForeignKey, Integer, String, Text, Table, UniqueConstraint
from sqlalchemy.orm import relationship

from kubarr.core.database import Base


# Many-to-many association table for users and roles
user_roles = Table(
    "user_roles",
    Base.metadata,
    Column("user_id", Integer, ForeignKey("users.id", ondelete="CASCADE"), primary_key=True),
    Column("role_id", Integer, ForeignKey("roles.id", ondelete="CASCADE"), primary_key=True),
)


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
    roles = relationship("Role", secondary=user_roles, back_populates="users", lazy="selectin")

    def has_role(self, role_name: str) -> bool:
        """Check if user has a specific role."""
        return any(role.name == role_name for role in self.roles)

    def has_admin_role(self) -> bool:
        """Check if user has admin role (via roles or legacy is_admin flag)."""
        return self.is_admin or self.has_role("admin")

    def get_allowed_apps(self) -> Optional[set]:
        """Get set of app names user can access. Returns None for admin (all apps)."""
        if self.has_admin_role():
            return None  # None means all apps

        allowed = set()
        for role in self.roles:
            for perm in role.app_permissions:
                allowed.add(perm.app_name)
        return allowed

    def can_access_app(self, app_name: str) -> bool:
        """Check if user can access a specific app."""
        allowed = self.get_allowed_apps()
        if allowed is None:  # Admin
            return True
        return app_name in allowed

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


class Invite(Base):
    """Invite model for one-time use registration links."""

    __tablename__ = "invites"

    id = Column(Integer, primary_key=True, index=True)
    code = Column(String(64), unique=True, nullable=False, index=True)
    created_by_id = Column(Integer, ForeignKey("users.id"), nullable=False, index=True)
    used_by_id = Column(Integer, ForeignKey("users.id"), nullable=True, index=True)
    is_used = Column(Boolean, default=False, nullable=False)
    expires_at = Column(DateTime, nullable=True, index=True)
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)
    used_at = Column(DateTime, nullable=True)

    # Relationships
    created_by = relationship("User", foreign_keys=[created_by_id], backref="created_invites")
    used_by = relationship("User", foreign_keys=[used_by_id], backref="used_invite")

    def __repr__(self) -> str:
        return f"<Invite(id={self.id}, code='{self.code[:8]}...', is_used={self.is_used})>"


class Role(Base):
    """Role model for access control."""

    __tablename__ = "roles"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String(50), unique=True, nullable=False, index=True)
    description = Column(String(255), nullable=True)
    is_system = Column(Boolean, default=False, nullable=False)  # Protect default roles from deletion
    created_at = Column(DateTime, default=datetime.utcnow, nullable=False)

    # Relationships
    users = relationship("User", secondary=user_roles, back_populates="roles")
    app_permissions = relationship("RoleAppPermission", back_populates="role", cascade="all, delete-orphan", lazy="selectin")

    def __repr__(self) -> str:
        return f"<Role(id={self.id}, name='{self.name}')>"


class RoleAppPermission(Base):
    """App permission for a role."""

    __tablename__ = "role_app_permissions"

    id = Column(Integer, primary_key=True, index=True)
    role_id = Column(Integer, ForeignKey("roles.id", ondelete="CASCADE"), nullable=False, index=True)
    app_name = Column(String(50), nullable=False, index=True)

    # Relationships
    role = relationship("Role", back_populates="app_permissions")

    __table_args__ = (
        UniqueConstraint('role_id', 'app_name', name='uq_role_app'),
    )

    def __repr__(self) -> str:
        return f"<RoleAppPermission(role_id={self.role_id}, app_name='{self.app_name}')>"


class SystemSettings(Base):
    """System settings stored in database for runtime configuration."""

    __tablename__ = "system_settings"

    key = Column(String(100), primary_key=True, index=True)
    value = Column(Text, nullable=False)
    description = Column(String(255), nullable=True)
    updated_at = Column(DateTime, default=datetime.utcnow, onupdate=datetime.utcnow, nullable=False)

    def __repr__(self) -> str:
        return f"<SystemSettings(key='{self.key}', value='{self.value}')>"
