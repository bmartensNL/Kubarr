"""Helm chart deployment for Kubarr."""

import json
import time
from pathlib import Path

from .config import (
    ALL_CHARTS,
    CORE_CHARTS,
    IMAGES,
    MONITORING_CHARTS,
    NAMESPACES,
    TIMEOUT_DEPLOY,
    TIMEOUT_POD_READY,
    get_charts_dir,
)
from .utils import log, run, run_quiet


def chart_exists(name: str) -> bool:
    """Check if a chart directory exists.

    Args:
        name: Chart name

    Returns:
        True if chart exists
    """
    chart_path = get_charts_dir() / name
    return chart_path.exists() and (chart_path / "Chart.yaml").exists()


def is_chart_installed(name: str, namespace: str = None) -> bool:
    """Check if a Helm release is installed.

    Args:
        name: Release name
        namespace: Kubernetes namespace

    Returns:
        True if release is installed
    """
    if namespace is None:
        namespace = NAMESPACES.get(name, name)

    # Use helm list to check if release exists (more reliable than helm status)
    success, output = run_quiet(
        f"helm list -n {namespace} -q --filter '^{name}$'",
        check=False,
    )
    return success and name in output.strip()


def install_chart(
    name: str,
    namespace: str = None,
    values_file: Path = None,
    extra_args: str = "",
    upgrade: bool = True,
) -> bool:
    """Install or upgrade a Helm chart.

    Args:
        name: Chart name
        namespace: Kubernetes namespace (defaults to chart name)
        values_file: Custom values file
        extra_args: Additional helm arguments
        upgrade: Whether to upgrade if already installed

    Returns:
        True if chart was installed/upgraded
    """
    if namespace is None:
        namespace = NAMESPACES.get(name, name)

    chart_path = get_charts_dir() / name
    if not chart_exists(name):
        log(f"Chart '{name}' not found at {chart_path}", "error")
        return False

    # Use helm upgrade --install which handles both install and upgrade
    log(f"Deploying {name}...")
    cmd = f"helm upgrade --install {name} {chart_path} -n {namespace} --create-namespace"

    if values_file and values_file.exists():
        cmd += f" -f {values_file}"

    if extra_args:
        cmd += f" {extra_args}"

    try:
        run(cmd, timeout=TIMEOUT_DEPLOY)
        log(f"Deployed {name}", "success")
        return True
    except Exception as e:
        log(f"Failed to deploy {name}: {e}", "error")
        return False


def uninstall_chart(name: str, namespace: str = None) -> bool:
    """Uninstall a Helm release.

    Args:
        name: Release name
        namespace: Kubernetes namespace

    Returns:
        True if release was uninstalled
    """
    if namespace is None:
        namespace = NAMESPACES.get(name, name)

    if not is_chart_installed(name, namespace):
        log(f"{name} is not installed", "warning")
        return True

    log(f"Uninstalling {name}...")
    try:
        run(f"helm uninstall {name} -n {namespace}")
        log(f"Uninstalled {name}", "success")
        return True
    except Exception as e:
        log(f"Failed to uninstall {name}: {e}", "error")
        return False


def wait_for_pods(
    namespace: str,
    label: str = None,
    timeout: int = TIMEOUT_POD_READY,
) -> bool:
    """Wait for pods to be ready in a namespace.

    Args:
        namespace: Kubernetes namespace
        label: Label selector (e.g., "app.kubernetes.io/name=kubarr")
        timeout: Timeout in seconds

    Returns:
        True if pods are ready
    """
    log(f"Waiting for pods in {namespace}...")

    cmd = f"kubectl wait --for=condition=Ready pod -n {namespace} --timeout={timeout}s"
    if label:
        cmd += f" -l {label}"
    else:
        cmd += " --all"

    success, output = run_quiet(cmd, check=False, timeout=timeout + 10)

    if success:
        log(f"Pods in {namespace} are ready", "success")
    else:
        log(f"Some pods in {namespace} may not be ready: {output}", "warning")

    return success


def deploy_core(
    images: dict[str, str] = None,
    skip_image_override: bool = False,
) -> bool:
    """Deploy core Kubarr charts (kubarr, nginx, oauth2-proxy).

    Args:
        images: Dictionary mapping component to image (for kubarr chart)
        skip_image_override: Don't override image settings

    Returns:
        True if all charts were deployed
    """
    for chart in CORE_CHARTS:
        if not chart_exists(chart):
            log(f"Chart '{chart}' not found, skipping", "warning")
            continue

        extra_args = ""

        # For kubarr chart, set image overrides
        if chart == "kubarr" and images and not skip_image_override:
            frontend_image = images.get("frontend", f"{IMAGES['frontend']}:latest")
            backend_image = images.get("backend", f"{IMAGES['backend']}:latest")

            extra_args = (
                f"--set backend.image.repository={backend_image.rsplit(':', 1)[0]} "
                f"--set backend.image.tag={backend_image.rsplit(':', 1)[1]} "
                f"--set backend.image.pullPolicy=Never "
                f"--set frontend.image.repository={frontend_image.rsplit(':', 1)[0]} "
                f"--set frontend.image.tag={frontend_image.rsplit(':', 1)[1]} "
                f"--set frontend.image.pullPolicy=Never"
            )

        if not install_chart(chart, extra_args=extra_args):
            return False

        # Wait for pods
        namespace = NAMESPACES[chart]
        wait_for_pods(namespace)

    return True


def deploy_monitoring() -> bool:
    """Deploy monitoring stack (prometheus, loki, promtail, grafana).

    Returns:
        True if all available charts were deployed
    """
    for chart in MONITORING_CHARTS:
        if not chart_exists(chart):
            log(f"Chart '{chart}' not found, skipping", "warning")
            continue

        if not install_chart(chart):
            log(f"Failed to install {chart}, continuing...", "warning")
            continue

        # Wait for pods
        namespace = NAMESPACES[chart]
        wait_for_pods(namespace, timeout=180)

    return True


def deploy_all(images: dict[str, str] = None, skip_monitoring: bool = False) -> bool:
    """Deploy all charts.

    Args:
        images: Dictionary mapping component to image
        skip_monitoring: Skip monitoring charts

    Returns:
        True if deployment succeeded
    """
    if not deploy_core(images):
        return False

    if not skip_monitoring:
        deploy_monitoring()

    return True


def get_chart_status() -> dict:
    """Get status of all Helm releases.

    Returns:
        Dictionary with chart status information
    """
    status = {}

    for chart in ALL_CHARTS:
        namespace = NAMESPACES.get(chart, chart)
        installed = is_chart_installed(chart, namespace)

        status[chart] = {
            "installed": installed,
            "namespace": namespace,
        }

        if installed:
            # Get release info
            success, output = run_quiet(
                f"helm status {chart} -n {namespace} -o json",
                check=False,
            )
            if success:
                try:
                    info = json.loads(output)
                    status[chart]["status"] = info.get("info", {}).get("status", "unknown")
                    status[chart]["version"] = info.get("version", 0)
                except json.JSONDecodeError:
                    status[chart]["status"] = "unknown"

    return status


def expose_oauth2_proxy(node_port: int = 30080) -> bool:
    """Expose oauth2-proxy via NodePort for Kind cluster access.

    Args:
        node_port: NodePort to use

    Returns:
        True if service was patched
    """
    namespace = NAMESPACES["oauth2-proxy"]
    log("Exposing oauth2-proxy on NodePort...")

    patch = (
        f'{{"spec": {{"type": "NodePort", "ports": '
        f'[{{"port": 4180, "targetPort": 4180, "nodePort": {node_port}}}]}}}}'
    )

    try:
        run(f"kubectl patch svc oauth2-proxy -n {namespace} -p '{patch}'")
        log(f"oauth2-proxy exposed on NodePort {node_port}", "success")
        return True
    except Exception as e:
        log(f"Failed to expose oauth2-proxy: {e}", "error")
        return False


def setup_initial_admin(
    admin_email: str = "admin@example.com",
    admin_password: str = "admin123",
) -> dict:
    """Complete initial setup via API.

    Args:
        admin_email: Admin email address
        admin_password: Admin password

    Returns:
        Setup response data or empty dict on failure
    """
    import subprocess

    log("Completing initial setup...")

    # Port forward to backend
    namespace = NAMESPACES["kubarr"]
    port_forward_proc = subprocess.Popen(
        f"kubectl port-forward -n {namespace} svc/kubarr 8000:8000",
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    try:
        time.sleep(3)

        # Check if setup is required
        success, output = run_quiet(
            "curl -s http://localhost:8000/api/setup/required",
            check=False,
        )
        if not success or '"setup_required":false' in output:
            log("Setup already completed or not required", "warning")
            return {}

        # Complete setup
        setup_data = json.dumps({
            "admin_username": "admin",
            "admin_email": admin_email,
            "admin_password": admin_password,
            "storage_path": "/data",
            "base_url": "http://localhost:8080",
        })

        success, output = run_quiet(
            f"curl -s -X POST http://localhost:8000/api/setup/initialize "
            f"-H 'Content-Type: application/json' "
            f"-d '{setup_data}'",
            check=False,
        )

        if success and '"success":true' in output:
            log("Admin user created", "success")
            try:
                result = json.loads(output)
                return result.get("data", {})
            except json.JSONDecodeError:
                return {}
        else:
            log(f"Setup failed: {output}", "warning")
            return {}
    finally:
        port_forward_proc.terminate()
        port_forward_proc.wait()
