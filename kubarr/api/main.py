"""FastAPI application for Kubarr dashboard."""

import os
from pathlib import Path

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles

from kubarr.api.config import settings
from kubarr.api.routers import apps, auth, login, logs, monitoring, proxy, setup, system, users
from kubarr.core.database import init_db, close_db

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

# Proxy router for deployed apps (must be AFTER api routes, BEFORE SPA catch-all)
app.include_router(proxy.router, tags=["proxy"])


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
