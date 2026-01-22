"""FastAPI application for Kubarr dashboard."""

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from kubarr.api.config import settings
from kubarr.api.routers import apps, auth, login, logs, monitoring, setup, system, users
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

# OAuth2 and authentication routers
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

# Setup router
app.include_router(
    setup.router,
    prefix="/api/setup",
    tags=["setup"]
)


# Root endpoint
@app.get("/")
async def root():
    """Root endpoint."""
    return {
        "name": settings.api_title,
        "version": settings.api_version,
        "docs": "/docs",
        "health": "/api/system/health"
    }


# Health check (alternative path)
@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy", "service": "kubarr-api"}
