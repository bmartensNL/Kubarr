"""Storage management API router for file browsing."""

import os
import shutil
from datetime import datetime
from pathlib import Path
from typing import List, Optional

from fastapi import APIRouter, Depends, HTTPException, Query, status
from fastapi.responses import FileResponse
from pydantic import BaseModel
from sqlalchemy.ext.asyncio import AsyncSession

from kubarr.api.dependencies import get_current_active_user, get_db
from kubarr.core.models_auth import User
from kubarr.core.setup import get_storage_path as get_storage_path_from_db

router = APIRouter()

# Fallback storage path if not configured in database
DEFAULT_STORAGE_PATH = os.environ.get("KUBARR_STORAGE_PATH", "/data")

# Protected top-level folders that cannot be deleted
PROTECTED_FOLDERS = {"downloads", "media"}


async def get_configured_storage_path(db: AsyncSession = Depends(get_db)) -> str:
    """Get the configured storage path for use inside the container.

    The storage path in the database is the node path (for Helm deployments).
    Inside the container, we use KUBARR_STORAGE_PATH which points to where
    the volume is mounted.

    Args:
        db: Database session

    Returns:
        Storage path string (the mount path inside the container)

    Raises:
        HTTPException: If storage is not configured
    """
    # Check if storage is configured in DB (validates setup is complete)
    db_path = await get_storage_path_from_db(db)

    # Use the environment variable for the actual path inside the container
    # This is where the hostPath volume is mounted
    if DEFAULT_STORAGE_PATH and Path(DEFAULT_STORAGE_PATH).exists():
        return DEFAULT_STORAGE_PATH

    # If no mount path but DB has a path, storage might not be mounted
    if db_path:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Storage is configured but not mounted. Check kubarr-dashboard deployment."
        )

    # Storage not configured at all
    raise HTTPException(
        status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
        detail="Storage not configured. Please complete initial setup."
    )


class FileInfo(BaseModel):
    """Information about a file or directory."""
    name: str
    path: str
    type: str  # "file" or "directory"
    size: int
    modified: datetime
    permissions: str


class DirectoryListing(BaseModel):
    """Directory listing response."""
    path: str
    parent: Optional[str]
    items: List[FileInfo]
    total_items: int


class StorageStats(BaseModel):
    """Storage usage statistics."""
    total_bytes: int
    used_bytes: int
    free_bytes: int
    usage_percent: float


class CreateDirectoryRequest(BaseModel):
    """Request to create a new directory."""
    path: str


class DeleteRequest(BaseModel):
    """Request to delete a file or directory."""
    path: str


def validate_path(requested_path: str, storage_path: str) -> Path:
    """Validate and resolve a requested path to prevent directory traversal.

    Args:
        requested_path: The path requested by the user
        storage_path: Base storage path

    Returns:
        Resolved absolute path

    Raises:
        HTTPException: If path is invalid or attempts traversal
    """
    # Get the base storage path
    base_path = Path(storage_path).resolve()

    # Normalize and resolve the requested path
    if requested_path.startswith("/"):
        # Absolute path within storage
        full_path = base_path / requested_path.lstrip("/")
    else:
        full_path = base_path / requested_path

    resolved = full_path.resolve()

    # Ensure the resolved path is within the storage base
    try:
        resolved.relative_to(base_path)
    except ValueError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Access denied: path traversal attempt detected"
        )

    return resolved


def get_file_info(file_path: Path, base_path: Path) -> FileInfo:
    """Get information about a file or directory.

    Args:
        file_path: Path to the file
        base_path: Base storage path for relative path calculation

    Returns:
        FileInfo object
    """
    stat = file_path.stat()
    relative_path = str(file_path.relative_to(base_path))

    # Calculate size (0 for directories, actual size for files)
    if file_path.is_dir():
        size = 0
        file_type = "directory"
    else:
        size = stat.st_size
        file_type = "file"

    # Get permissions as octal string
    permissions = oct(stat.st_mode)[-3:]

    return FileInfo(
        name=file_path.name,
        path=relative_path,
        type=file_type,
        size=size,
        modified=datetime.fromtimestamp(stat.st_mtime),
        permissions=permissions
    )


@router.get("/browse", response_model=DirectoryListing)
async def browse_directory(
    path: str = Query(default="", description="Path relative to storage root"),
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> DirectoryListing:
    """Browse a directory in the shared storage.

    Args:
        path: Path relative to storage root
        storage_path: Configured storage path from database

    Returns:
        DirectoryListing with files and directories
    """
    base_path = Path(storage_path).resolve()

    # Handle empty path as root
    if not path or path == "/":
        dir_path = base_path
    else:
        dir_path = validate_path(path, storage_path)

    if not dir_path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Path not found: {path}"
        )

    if not dir_path.is_dir():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Path is not a directory: {path}"
        )

    # List directory contents
    items = []
    try:
        for entry in sorted(dir_path.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            try:
                items.append(get_file_info(entry, base_path))
            except (PermissionError, OSError):
                # Skip files we can't access
                continue
    except PermissionError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Permission denied"
        )

    # Calculate relative path and parent
    relative_path = str(dir_path.relative_to(base_path))
    if relative_path == ".":
        relative_path = ""
        parent = None
    else:
        parent_path = dir_path.parent
        if parent_path == base_path:
            parent = ""
        else:
            parent = str(parent_path.relative_to(base_path))

    return DirectoryListing(
        path=relative_path,
        parent=parent,
        items=items,
        total_items=len(items)
    )


@router.get("/stats", response_model=StorageStats)
async def get_storage_stats(
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> StorageStats:
    """Get storage usage statistics.

    Returns:
        StorageStats with total, used, and free space
    """
    base_path = Path(storage_path)

    if not base_path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Storage path not found"
        )

    try:
        usage = shutil.disk_usage(base_path)
        usage_percent = (usage.used / usage.total) * 100 if usage.total > 0 else 0

        return StorageStats(
            total_bytes=usage.total,
            used_bytes=usage.used,
            free_bytes=usage.free,
            usage_percent=round(usage_percent, 2)
        )
    except OSError as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get storage stats: {str(e)}"
        )


@router.get("/file-info", response_model=FileInfo)
async def get_file_details(
    path: str = Query(..., description="Path to file or directory"),
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> FileInfo:
    """Get detailed information about a specific file or directory.

    Args:
        path: Path relative to storage root
        storage_path: Configured storage path from database

    Returns:
        FileInfo with file details
    """
    file_path = validate_path(path, storage_path)
    base_path = Path(storage_path).resolve()

    if not file_path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Path not found: {path}"
        )

    try:
        return get_file_info(file_path, base_path)
    except PermissionError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Permission denied"
        )


@router.post("/mkdir")
async def create_directory(
    request: CreateDirectoryRequest,
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> dict:
    """Create a new directory.

    Args:
        request: CreateDirectoryRequest with path
        storage_path: Configured storage path from database

    Returns:
        Success status

    Note:
        Only admin users can create directories.
    """
    # Check admin permission
    if not current_user.is_admin:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Only administrators can create directories"
        )

    dir_path = validate_path(request.path, storage_path)

    if dir_path.exists():
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=f"Path already exists: {request.path}"
        )

    try:
        dir_path.mkdir(parents=True, exist_ok=False)
        # Set permissions to 775 (rwxrwxr-x)
        os.chmod(dir_path, 0o775)
        return {"success": True, "message": f"Directory created: {request.path}"}
    except PermissionError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Permission denied"
        )
    except OSError as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to create directory: {str(e)}"
        )


@router.delete("/delete")
async def delete_path(
    path: str = Query(..., description="Path to delete"),
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> dict:
    """Delete a file or empty directory.

    Args:
        path: Path relative to storage root
        storage_path: Configured storage path from database

    Returns:
        Success status

    Note:
        - Only admin users can delete
        - Protected top-level folders cannot be deleted
        - Non-empty directories cannot be deleted
    """
    # Check admin permission
    if not current_user.is_admin:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Only administrators can delete files"
        )

    target_path = validate_path(path, storage_path)
    base_path = Path(storage_path).resolve()

    if not target_path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Path not found: {path}"
        )

    # Check if trying to delete the root
    if target_path == base_path:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Cannot delete the storage root"
        )

    # Check if trying to delete a protected top-level folder
    relative_path = target_path.relative_to(base_path)
    parts = relative_path.parts
    if len(parts) == 1 and parts[0] in PROTECTED_FOLDERS:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail=f"Cannot delete protected folder: {parts[0]}"
        )

    try:
        if target_path.is_dir():
            # Only delete empty directories
            if any(target_path.iterdir()):
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail="Cannot delete non-empty directory"
                )
            target_path.rmdir()
        else:
            target_path.unlink()

        return {"success": True, "message": f"Deleted: {path}"}
    except PermissionError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Permission denied"
        )
    except OSError as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to delete: {str(e)}"
        )


@router.get("/download")
async def download_file(
    path: str = Query(..., description="Path to file to download"),
    current_user: User = Depends(get_current_active_user),
    storage_path: str = Depends(get_configured_storage_path)
) -> FileResponse:
    """Download a file from storage.

    Args:
        path: Path relative to storage root
        current_user: Authenticated user
        storage_path: Configured storage path

    Returns:
        File download response
    """
    file_path = validate_path(path, storage_path)

    if not file_path.exists():
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"File not found: {path}"
        )

    if file_path.is_dir():
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Cannot download a directory"
        )

    try:
        return FileResponse(
            path=str(file_path),
            filename=file_path.name,
            media_type="application/octet-stream"
        )
    except PermissionError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Permission denied"
        )
    except OSError as e:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to download file: {str(e)}"
        )
