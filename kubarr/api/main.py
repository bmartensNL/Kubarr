"""FastAPI application for Kubarr dashboard."""

import os
import subprocess
from pathlib import Path

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles

from kubarr.api.config import settings
from kubarr.api.routers import apps, auth, login, logs, monitoring, roles, setup, storage, system, users
from kubarr.api.routers import settings as settings_router
from kubarr.core.database import init_db, close_db
from kubarr.core.app_catalog import AppCatalog

# Path to charts directory
CHARTS_DIR = Path(os.environ.get("CHARTS_DIR", "/app/charts"))

# Create FastAPI app
app = FastAPI(
    title=settings.api_title,
    description=settings.api_description,
    version=settings.api_version,
    docs_url="/docs",
    redoc_url="/redoc",
    openapi_url="/openapi.json",
)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_origins,
    allow_credentials=settings.cors_allow_credentials,
    allow_methods=settings.cors_allow_methods,
    allow_headers=settings.cors_allow_headers,
)


# Exception handlers
@app.exception_handler(404)
async def not_found_handler(request, exc):
    """Handle 404 errors."""
    return JSONResponse(
        status_code=404,
        content={"detail": "Not found"}
    )


@app.exception_handler(500)
async def internal_error_handler(request, exc):
    """Handle 500 errors."""
    return JSONResponse(
        status_code=500,
        content={"detail": "Internal server error"}
    )


def ensure_system_apps_deployed():
    """Ensure all system apps are deployed on startup.

    This checks for system apps (kubarr.io/system: true) and deploys
    any that are not already installed.
    """
    try:
        catalog = AppCatalog()
        system_apps = [app for app in catalog.get_all_apps() if app.is_system]

        if not system_apps:
            print("No system apps found in catalog")
            return

        # Get list of installed helm releases
        result = subprocess.run(
            ["helm", "list", "-A", "-q"],
            capture_output=True,
            text=True
        )
        installed_releases = set(result.stdout.strip().split("\n")) if result.stdout.strip() else set()

        for app in system_apps:
            if app.name in installed_releases:
                print(f"System app '{app.name}' already installed")
                continue

            chart_path = CHARTS_DIR / app.name
            if not chart_path.exists():
                print(f"Warning: Chart not found for system app '{app.name}'")
                continue

            # Deploy the system app
            print(f"Installing system app '{app.name}'...")
            try:
                deploy_result = subprocess.run(
                    [
                        "helm", "install", app.name,
                        str(chart_path),
                        "-n", app.name,
                        "--create-namespace"
                    ],
                    capture_output=True,
                    text=True
                )
                if deploy_result.returncode == 0:
                    print(f"Successfully installed system app '{app.name}'")
                else:
                    print(f"Failed to install '{app.name}': {deploy_result.stderr}")
            except Exception as e:
                print(f"Error installing '{app.name}': {e}")

    except Exception as e:
        print(f"Warning: Failed to ensure system apps: {e}")


# Startup event
@app.on_event("startup")
async def startup_event():
    """Run on application startup."""
    print(f"Starting {settings.api_title} v{settings.api_version}")
    print(f"API documentation available at: http://{settings.api_host}:{settings.api_port}/docs")
    print(f"In-cluster mode: {settings.in_cluster}")
    print(f"Default namespace: {settings.default_namespace}")

    # Initialize database if OAuth2 is enabled
    if settings.oauth2_enabled:
        print("OAuth2 is enabled, initializing database...")
        init_db()
        print("Database initialized")

        # Sync OAuth2 credentials to Kubernetes secret on startup
        if settings.in_cluster:
            try:
                from kubarr.core.database import async_session_maker
                from kubarr.core.setup import sync_oauth2_client_on_startup

                async with async_session_maker() as db:
                    result = await sync_oauth2_client_on_startup(db)
                    if result:
                        print("OAuth2 credentials synced to Kubernetes")
                    else:
                        print("OAuth2 credentials sync skipped or failed")
            except Exception as e:
                print(f"Warning: Failed to sync OAuth2 credentials: {e}")

    # Ensure system apps are deployed (only in-cluster)
    if settings.in_cluster:
        print("Ensuring system apps are deployed...")
        ensure_system_apps_deployed()


# Shutdown event
@app.on_event("shutdown")
async def shutdown_event():
    """Run on application shutdown."""
    print(f"Shutting down {settings.api_title}")

    # Close database connections
    if settings.oauth2_enabled:
        await close_db()


# Include routers
app.include_router(
    apps.router,
    prefix="/api/apps",
    tags=["apps"]
)

app.include_router(
    monitoring.router,
    prefix="/api/monitoring",
    tags=["monitoring"]
)

app.include_router(
    logs.router,
    prefix="/api/logs",
    tags=["logs"]
)

app.include_router(
    system.router,
    prefix="/api/system",
    tags=["system"]
)

app.include_router(
    users.router,
    prefix="/api/users",
    tags=["users"]
)

app.include_router(
    roles.router,
    prefix="/api/roles",
    tags=["roles"]
)

app.include_router(
    settings_router.router,
    prefix="/api/settings",
    tags=["settings"]
)

app.include_router(
    storage.router,
    prefix="/api/storage",
    tags=["storage"]
)

# Setup router
app.include_router(
    setup.router,
    prefix="/api/setup",
    tags=["setup"]
)

# OAuth2 and authentication routers (must be before proxy to avoid catching auth paths)
app.include_router(
    auth.router,
    prefix="/auth",
    tags=["auth"]
)

app.include_router(
    login.router,
    prefix="/auth",
    tags=["login"]
)


# Health check (alternative path)
@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy", "service": "kubarr-api"}


# Test endpoint with no dependencies
@app.get("/api/test")
async def test_endpoint():
    """Test endpoint with no dependencies."""
    print("[TEST] Test endpoint called!")
    return {"status": "ok", "message": "Test endpoint works"}


# Serve static assets
static_dir = os.getenv("STATIC_FILES_DIR", "/app/static")
if os.path.exists(static_dir):
    assets_dir = os.path.join(static_dir, "assets")
    if os.path.exists(assets_dir):
        app.mount("/assets", StaticFiles(directory=assets_dir), name="assets")

# NOTE: App proxying CANNOT be done by kubarr-dashboard backend.
# Apps must be accessed through their own ingress/service endpoints.
# See claude.md for architecture details.


# Serve SPA (must be LAST - catch-all for SPA routing)
if os.path.exists(static_dir):
    from fastapi.responses import FileResponse

    @app.get("/{full_path:path}")
    async def serve_spa(full_path: str):
        """Serve SPA index.html for all unmatched routes."""
        index_path = Path(static_dir) / "index.html"
        if index_path.is_file():
            return FileResponse(index_path, media_type="text/html")
        return JSONResponse({"detail": "Not found"}, status_code=404)
