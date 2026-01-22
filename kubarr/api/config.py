"""API configuration."""

import os
from typing import List

from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    """API configuration settings."""

    # API settings
    api_host: str = "0.0.0.0"
    api_port: int = 8000
    api_title: str = "Kubarr API"
    api_version: str = "0.1.0"
    api_description: str = "API for managing Kubernetes media stack"

    # CORS settings
    cors_origins: List[str] = ["http://localhost:5173", "http://localhost:3000"]
    cors_allow_credentials: bool = True
    cors_allow_methods: List[str] = ["*"]
    cors_allow_headers: List[str] = ["*"]

    # Kubernetes settings
    kubeconfig_path: str | None = None
    in_cluster: bool = False
    default_namespace: str = "media"

    # Feature flags
    enable_metrics: bool = True
    enable_websocket: bool = True

    # Logging
    log_level: str = "INFO"

    # Database settings
    db_path: str = "/data/kubarr.db"

    # OAuth2 settings
    oauth2_enabled: bool = False
    oauth2_issuer_url: str = "http://kubarr-dashboard:8000"

    # JWT settings
    jwt_algorithm: str = "RS256"
    jwt_private_key_path: str = "/secrets/jwt-private.pem"
    jwt_public_key_path: str = "/secrets/jwt-public.pem"
    jwt_access_token_expire: int = 3600  # 1 hour
    jwt_refresh_token_expire: int = 604800  # 7 days

    # Admin user (created on first run)
    admin_username: str = "admin"
    admin_password: str = ""  # Generated if empty
    admin_email: str = "admin@kubarr.local"

    # Registration settings
    registration_enabled: bool = True
    registration_require_approval: bool = True

    class Config:
        """Pydantic config."""

        env_prefix = "KUBARR_"
        case_sensitive = False


# Global settings instance
settings = Settings()
