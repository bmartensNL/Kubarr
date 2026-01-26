"""Docker image building and loading for Kubarr deployment."""

import subprocess
import time
from pathlib import Path

from .config import (
    COMPONENTS,
    CONTAINERS,
    DEPLOYMENTS,
    DOCKERFILES,
    IMAGES,
    NAMESPACES,
    TIMEOUT_BUILD,
    get_project_root,
)
from .utils import log, run, run_quiet


def get_timestamp_tag() -> str:
    """Generate a timestamp-based tag for unique image identification.

    Returns:
        Timestamp tag (e.g., "1706025600")
    """
    return str(int(time.time()))


def get_git_commit() -> str:
    """Get the current git commit hash.

    Returns:
        Short commit hash or "unknown"
    """
    success, output = run_quiet("git rev-parse --short HEAD", check=False)
    return output.strip() if success else "unknown"


def get_build_time() -> str:
    """Get the current build time in ISO format.

    Returns:
        ISO formatted timestamp
    """
    from datetime import datetime, timezone
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def build_image(
    component: str,
    tag: str = None,
    project_root: Path = None,
) -> tuple[bool, str]:
    """Build a Docker image for a component.

    Args:
        component: Component name (frontend or backend)
        tag: Image tag (defaults to timestamp)
        project_root: Project root directory

    Returns:
        Tuple of (success, full_image_name)
    """
    if component not in COMPONENTS:
        log(f"Unknown component: {component}", "error")
        return False, ""

    if project_root is None:
        project_root = get_project_root()

    if tag is None:
        tag = get_timestamp_tag()

    image_name = IMAGES[component]
    dockerfile = project_root / DOCKERFILES[component]
    full_image = f"{image_name}:{tag}"

    log(f"Building {component} image ({full_image})...")

    # Get build args
    commit = get_git_commit()
    build_time = get_build_time()

    cmd = (
        f"docker build "
        f"-f {dockerfile} "
        f"-t {full_image} "
        f"--build-arg COMMIT_HASH={commit} "
        f"--build-arg BUILD_TIME={build_time} "
        f"{project_root}"
    )

    try:
        run(cmd, timeout=TIMEOUT_BUILD)
        log(f"Built {full_image}", "success")
        return True, full_image
    except subprocess.CalledProcessError as e:
        log(f"Failed to build {component}: {e}", "error")
        return False, ""
    except subprocess.TimeoutExpired:
        log(f"Build timed out for {component}", "error")
        return False, ""


def build_all(tag: str = None, project_root: Path = None) -> dict[str, str]:
    """Build all Docker images.

    Args:
        tag: Image tag (defaults to timestamp)
        project_root: Project root directory

    Returns:
        Dictionary mapping component to full image name
    """
    if tag is None:
        tag = get_timestamp_tag()

    results = {}
    for component in COMPONENTS:
        success, full_image = build_image(component, tag, project_root)
        if success:
            results[component] = full_image
        else:
            log(f"Failed to build {component}, aborting", "error")
            return {}

    return results


def load_image_to_kind(image: str, cluster_name: str) -> bool:
    """Load a Docker image into a Kind cluster.

    Uses docker save | ctr import instead of kind load for reliable loading.

    Args:
        image: Full image name (e.g., kubarr-frontend:1234567890)
        cluster_name: Kind cluster name

    Returns:
        True if image was loaded successfully
    """
    log(f"Loading {image} into Kind cluster...")

    # Use ctr import for reliable loading (per CLAUDE.md)
    cmd = (
        f"docker save {image} | "
        f"docker exec -i {cluster_name}-control-plane "
        f"ctr -n k8s.io images import -"
    )

    try:
        run(cmd, timeout=300)
        log(f"Loaded {image}", "success")
        return True
    except Exception as e:
        log(f"Failed to load {image}: {e}", "error")
        return False


def load_images_to_kind(images: dict[str, str], cluster_name: str) -> bool:
    """Load multiple Docker images into a Kind cluster.

    Args:
        images: Dictionary mapping component to full image name
        cluster_name: Kind cluster name

    Returns:
        True if all images were loaded
    """
    for component, image in images.items():
        if not load_image_to_kind(image, cluster_name):
            return False
    return True


def update_deployment_image(
    component: str,
    image: str,
    wait: bool = True,
) -> bool:
    """Update a deployment to use a new image and restart pods.

    Args:
        component: Component name (frontend or backend)
        image: Full image name
        wait: Whether to wait for rollout

    Returns:
        True if deployment was updated
    """
    if component not in COMPONENTS:
        log(f"Unknown component: {component}", "error")
        return False

    deployment = DEPLOYMENTS[component]
    container = CONTAINERS[component]
    namespace = NAMESPACES["kubarr"]  # Both frontend and backend are in kubarr namespace

    log(f"Updating {deployment} to use {image}...")

    try:
        # Update the image
        run(f"kubectl set image deployment/{deployment} {container}={image} -n {namespace}")

        # Delete pods to force recreation with new image
        run(f"kubectl delete pod -l app.kubernetes.io/name={deployment} -n {namespace}")

        if wait:
            # Wait for rollout
            run(f"kubectl rollout status deployment/{deployment} -n {namespace} --timeout=120s")

        log(f"Updated {deployment}", "success")
        return True
    except Exception as e:
        log(f"Failed to update {deployment}: {e}", "error")
        return False


def get_current_images() -> dict[str, str]:
    """Get the current images used by deployments.

    Returns:
        Dictionary mapping component to current image
    """
    images = {}
    namespace = NAMESPACES["kubarr"]

    for component in COMPONENTS:
        deployment = DEPLOYMENTS[component]
        container = CONTAINERS[component]

        success, output = run_quiet(
            f"kubectl get deployment/{deployment} -n {namespace} "
            f"-o jsonpath='{{.spec.template.spec.containers[?(@.name==\"{container}\")].image}}'",
            check=False,
        )
        if success:
            images[component] = output.strip()

    return images
