"""Kind cluster management for Kubarr deployment."""

import tempfile
from pathlib import Path

from .config import DEFAULT_CLUSTER_NAME, DEFAULT_HOST_PORT
from .utils import log, run, run_quiet


def cluster_exists(name: str = DEFAULT_CLUSTER_NAME) -> bool:
    """Check if a Kind cluster exists.

    Args:
        name: Cluster name

    Returns:
        True if cluster exists
    """
    success, output = run_quiet("kind get clusters", check=False)
    return success and name in output.split()


def create_cluster(
    name: str = DEFAULT_CLUSTER_NAME,
    host_port: int = DEFAULT_HOST_PORT,
) -> bool:
    """Create a Kind cluster with port mapping.

    Args:
        name: Cluster name
        host_port: Host port to map for NodePort services

    Returns:
        True if cluster was created or already exists
    """
    if cluster_exists(name):
        log(f"Cluster '{name}' already exists", "warning")
        return True

    log(f"Creating Kind cluster '{name}'...")

    # Kind cluster config with port mapping
    config = f"""kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  extraPortMappings:
  - containerPort: 30080
    hostPort: {host_port}
    protocol: TCP
"""

    # Write config to temp file
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yaml", delete=False
    ) as f:
        f.write(config)
        config_path = f.name

    try:
        run(f"kind create cluster --name {name} --config {config_path}", timeout=300)
        log(f"Cluster '{name}' created successfully", "success")
        return True
    except Exception as e:
        log(f"Failed to create cluster: {e}", "error")
        return False
    finally:
        Path(config_path).unlink(missing_ok=True)


def delete_cluster(name: str = DEFAULT_CLUSTER_NAME) -> bool:
    """Delete a Kind cluster.

    Args:
        name: Cluster name

    Returns:
        True if cluster was deleted or didn't exist
    """
    if not cluster_exists(name):
        log(f"Cluster '{name}' does not exist", "warning")
        return True

    log(f"Deleting cluster '{name}'...")
    try:
        run(f"kind delete cluster --name {name}")
        log(f"Cluster '{name}' deleted", "success")
        return True
    except Exception as e:
        log(f"Failed to delete cluster: {e}", "error")
        return False


def switch_context(name: str = DEFAULT_CLUSTER_NAME) -> bool:
    """Switch kubectl context to the Kind cluster.

    Args:
        name: Cluster name

    Returns:
        True if context was switched
    """
    context = f"kind-{name}"
    success, _ = run_quiet(f"kubectl config use-context {context}", check=False)
    if success:
        log(f"Switched to context '{context}'", "success")
    else:
        log(f"Failed to switch to context '{context}'", "error")
    return success


def get_cluster_info(name: str = DEFAULT_CLUSTER_NAME) -> dict:
    """Get information about the Kind cluster.

    Args:
        name: Cluster name

    Returns:
        Dictionary with cluster information
    """
    info = {
        "name": name,
        "exists": cluster_exists(name),
        "context": f"kind-{name}",
    }

    if info["exists"]:
        # Get node info
        success, output = run_quiet(
            f"kubectl get nodes -o jsonpath='{{.items[0].status.conditions[-1].type}}'"
            f" --context kind-{name}",
            check=False,
        )
        info["node_ready"] = success and "Ready" in output

        # Get control plane container ID
        success, output = run_quiet(
            f"docker ps -q -f name={name}-control-plane",
            check=False,
        )
        info["container_running"] = success and bool(output.strip())

    return info


def wait_for_cluster_ready(name: str = DEFAULT_CLUSTER_NAME, timeout: int = 60) -> bool:
    """Wait for the Kind cluster to be ready.

    Args:
        name: Cluster name
        timeout: Timeout in seconds

    Returns:
        True if cluster is ready
    """
    log("Waiting for cluster to be ready...")

    success, _ = run_quiet(
        f"kubectl wait --for=condition=Ready node --all --timeout={timeout}s "
        f"--context kind-{name}",
        check=False,
        timeout=timeout + 10,
    )

    if success:
        log("Cluster is ready", "success")
    else:
        log("Cluster not ready within timeout", "warning")

    return success
