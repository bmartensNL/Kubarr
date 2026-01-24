#!/usr/bin/env python3
"""Kubarr Setup Tool - Automated deployment for development and testing."""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


# Colors for terminal output
class Colors:
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    BLUE = "\033[94m"
    BOLD = "\033[1m"
    END = "\033[0m"


def log(msg: str, level: str = "info"):
    """Print colored log message."""
    colors = {
        "info": Colors.BLUE,
        "success": Colors.GREEN,
        "warning": Colors.YELLOW,
        "error": Colors.RED,
    }
    color = colors.get(level, Colors.BLUE)
    prefix = {"info": "ℹ", "success": "✓", "warning": "⚠", "error": "✗"}.get(level, "•")
    print(f"{color}{prefix}{Colors.END} {msg}")


def run(cmd: str, check: bool = True, capture: bool = False, timeout: int = 300) -> subprocess.CompletedProcess:
    """Run a shell command."""
    result = subprocess.run(
        cmd,
        shell=True,
        check=check,
        capture_output=capture,
        text=True,
        timeout=timeout,
    )
    return result


def run_quiet(cmd: str, check: bool = True, timeout: int = 300) -> tuple[bool, str]:
    """Run command and return success status and output."""
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            check=check,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return True, result.stdout
    except subprocess.CalledProcessError as e:
        return False, e.stderr or e.stdout or str(e)
    except subprocess.TimeoutExpired:
        return False, "Command timed out"


def check_prerequisites():
    """Check that required tools are installed."""
    log("Checking prerequisites...")

    tools = {
        "docker": "docker --version",
        "kubectl": "kubectl version --client",
        "helm": "helm version --short",
        "kind": "kind --version",
    }

    missing = []
    for tool, cmd in tools.items():
        success, _ = run_quiet(cmd, check=False)
        if success:
            log(f"  {tool} found", "success")
        else:
            log(f"  {tool} not found", "error")
            missing.append(tool)

    if missing:
        log(f"Missing required tools: {', '.join(missing)}", "error")
        sys.exit(1)


def cluster_exists(name: str) -> bool:
    """Check if a kind cluster exists."""
    success, output = run_quiet(f"kind get clusters", check=False)
    return success and name in output.split()


def create_cluster(name: str, host_port: int = 8080):
    """Create a kind cluster with port mapping."""
    if cluster_exists(name):
        log(f"Cluster '{name}' already exists", "warning")
        return

    log(f"Creating kind cluster '{name}'...")

    config = f"""
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  extraPortMappings:
  - containerPort: 30080
    hostPort: {host_port}
    protocol: TCP
"""

    # Write config to temp file
    config_path = Path("/tmp/kind-config.yaml")
    config_path.write_text(config)

    run(f"kind create cluster --name {name} --config {config_path}", timeout=300)
    log(f"Cluster '{name}' created", "success")


def delete_cluster(name: str):
    """Delete a kind cluster."""
    if not cluster_exists(name):
        log(f"Cluster '{name}' does not exist", "warning")
        return

    log(f"Deleting cluster '{name}'...")
    run(f"kind delete cluster --name {name}")
    log(f"Cluster '{name}' deleted", "success")


def build_images(project_root: Path):
    """Build Docker images."""
    log("Building Docker images...")

    # Backend
    log("  Building backend image...")
    run(f"docker build -t kubarr-backend:latest -f {project_root}/docker/Dockerfile.backend {project_root}", timeout=600)

    # Frontend
    log("  Building frontend image...")
    run(f"docker build -t kubarr-frontend:latest -f {project_root}/docker/Dockerfile.frontend {project_root}", timeout=600)

    log("Docker images built", "success")


def load_images(cluster_name: str):
    """Load Docker images into kind cluster."""
    log("Loading images into kind cluster...")
    run(f"kind load docker-image kubarr-backend:latest kubarr-frontend:latest --name {cluster_name}", timeout=300)
    log("Images loaded", "success")


def create_namespace(name: str):
    """Create a Kubernetes namespace if it doesn't exist."""
    success, _ = run_quiet(f"kubectl get namespace {name}", check=False)
    if not success:
        run(f"kubectl create namespace {name}")


def install_helm_chart(name: str, chart_path: Path, namespace: str, values_file: Path = None, extra_args: str = ""):
    """Install a Helm chart."""
    log(f"  Installing {name}...")

    # Check if already installed
    success, _ = run_quiet(f"helm status {name} -n {namespace}", check=False)
    if success:
        log(f"    {name} already installed, upgrading...", "warning")
        cmd = f"helm upgrade {name} {chart_path} -n {namespace}"
    else:
        cmd = f"helm install {name} {chart_path} -n {namespace} --create-namespace"

    if values_file and values_file.exists():
        cmd += f" -f {values_file}"

    if extra_args:
        cmd += f" {extra_args}"

    run(cmd)
    log(f"    {name} installed", "success")


def wait_for_pods(namespace: str, label: str, timeout: int = 120):
    """Wait for pods to be ready."""
    log(f"  Waiting for pods in {namespace}...")
    success, _ = run_quiet(
        f"kubectl wait --for=condition=Ready pod -l {label} -n {namespace} --timeout={timeout}s",
        check=False,
        timeout=timeout + 10
    )
    if success:
        log(f"    Pods in {namespace} ready", "success")
    else:
        log(f"    Timeout waiting for pods in {namespace}", "warning")


def deploy_kubarr(project_root: Path):
    """Deploy Kubarr dashboard and oauth2-proxy."""
    log("Deploying Kubarr...")

    charts_dir = project_root / "charts"

    # Kubarr Dashboard
    install_helm_chart(
        "kubarr-dashboard",
        charts_dir / "kubarr-dashboard",
        "kubarr-system",
        extra_args=(
            "--set backend.image.repository=kubarr-backend "
            "--set backend.image.tag=latest "
            "--set backend.image.pullPolicy=Never "
            "--set frontend.image.repository=kubarr-frontend "
            "--set frontend.image.tag=latest "
            "--set frontend.image.pullPolicy=Never "
            "--set storage.hostPath.enabled=false "
            "--set namespace.create=false"
        )
    )

    # OAuth2 Proxy
    install_helm_chart(
        "oauth2-proxy",
        charts_dir / "oauth2-proxy",
        "kubarr-system",
    )

    # Wait for dashboard to be ready
    wait_for_pods("kubarr-system", "app.kubernetes.io/name=kubarr-dashboard")


def deploy_monitoring(project_root: Path):
    """Deploy monitoring stack (Loki, Promtail, Prometheus, Grafana)."""
    log("Deploying monitoring stack...")

    charts_dir = project_root / "charts"

    # Loki
    if (charts_dir / "loki").exists():
        install_helm_chart(
            "loki",
            charts_dir / "loki",
            "loki",
            charts_dir / "loki" / "values.yaml"
        )
        wait_for_pods("loki", "app.kubernetes.io/name=loki")

    # Promtail
    if (charts_dir / "promtail").exists():
        install_helm_chart(
            "promtail",
            charts_dir / "promtail",
            "promtail",
            charts_dir / "promtail" / "values.yaml"
        )
        wait_for_pods("promtail", "app.kubernetes.io/name=promtail")

    # Prometheus
    if (charts_dir / "prometheus").exists():
        install_helm_chart(
            "prometheus",
            charts_dir / "prometheus",
            "prometheus",
            charts_dir / "prometheus" / "values.yaml"
        )
        wait_for_pods("prometheus", "app.kubernetes.io/name=prometheus", timeout=180)

    # Grafana
    if (charts_dir / "grafana").exists():
        install_helm_chart(
            "grafana",
            charts_dir / "grafana",
            "grafana",
            charts_dir / "grafana" / "values.yaml"
        )
        wait_for_pods("grafana", "app.kubernetes.io/name=grafana")


def setup_initial_admin(admin_email: str = "admin@example.com", admin_password: str = "admin123"):
    """Complete initial setup via API."""
    log("Completing initial setup...")

    # Wait for backend to be ready
    time.sleep(5)

    # Port forward to backend
    import threading

    port_forward_proc = subprocess.Popen(
        "kubectl port-forward -n kubarr-system svc/kubarr-dashboard 8000:8000",
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    try:
        time.sleep(3)

        # Check if setup is required
        success, output = run_quiet("curl -s http://localhost:8000/api/setup/required", check=False)
        if not success or '"setup_required":false' in output:
            log("  Setup already completed or not required", "warning")
            return None

        # Complete setup
        setup_data = {
            "admin_username": "admin",
            "admin_email": admin_email,
            "admin_password": admin_password,
            "storage_path": "/data",
            "base_url": "http://localhost:8080"
        }

        success, output = run_quiet(
            f"curl -s -X POST http://localhost:8000/api/setup/initialize "
            f"-H 'Content-Type: application/json' "
            f"-d '{json.dumps(setup_data)}'",
            check=False
        )

        if success and '"success":true' in output:
            log("  Admin user created", "success")
            result = json.loads(output)
            return result.get("data", {})
        else:
            log(f"  Setup failed: {output}", "error")
            return None
    finally:
        port_forward_proc.terminate()
        port_forward_proc.wait()


def expose_service():
    """Expose oauth2-proxy via NodePort."""
    log("Exposing dashboard service...")
    run(
        "kubectl patch svc oauth2-proxy -n kubarr-system "
        "-p '{\"spec\": {\"type\": \"NodePort\", \"ports\": [{\"port\": 80, \"targetPort\": 4180, \"nodePort\": 30080}]}}'"
    )
    log("  Service exposed on NodePort 30080", "success")


def wait_for_oauth2_proxy():
    """Wait for oauth2-proxy to be ready (after setup creates the secret)."""
    log("Waiting for oauth2-proxy...")

    for i in range(60):
        success, output = run_quiet(
            "kubectl get pod -n kubarr-system -l app.kubernetes.io/name=oauth2-proxy -o jsonpath='{.items[0].status.phase}'",
            check=False
        )
        if success and "Running" in output:
            # Check if actually ready
            success2, _ = run_quiet(
                "kubectl wait --for=condition=Ready pod -l app.kubernetes.io/name=oauth2-proxy -n kubarr-system --timeout=5s",
                check=False
            )
            if success2:
                log("  oauth2-proxy ready", "success")
                return
        time.sleep(2)

    log("  oauth2-proxy may not be fully ready", "warning")


def print_summary(admin_email: str, admin_password: str, host_port: int):
    """Print setup summary."""
    print()
    print(f"{Colors.BOLD}{'='*60}{Colors.END}")
    print(f"{Colors.GREEN}{Colors.BOLD}Kubarr Setup Complete!{Colors.END}")
    print(f"{'='*60}")
    print()
    print(f"{Colors.BOLD}Dashboard URL:{Colors.END} http://localhost:{host_port}")
    print()
    print(f"{Colors.BOLD}Login Credentials:{Colors.END}")
    print(f"  Email:    {admin_email}")
    print(f"  Password: {admin_password}")
    print()
    print(f"{Colors.BOLD}Installed Components:{Colors.END}")
    print("  • Kubarr Dashboard (backend + frontend)")
    print("  • OAuth2 Proxy (authentication)")
    print("  • Loki (log aggregation)")
    print("  • Promtail (log collection)")
    print("  • Prometheus (metrics) - if chart exists")
    print("  • Grafana (dashboards) - if chart exists")
    print()
    print(f"{Colors.YELLOW}Note:{Colors.END} Apps installed via the UI will create their own namespaces.")
    print(f"{'='*60}")


def main():
    parser = argparse.ArgumentParser(
        description="Kubarr Setup Tool - Deploy Kubarr for development/testing",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Full setup with new cluster
  python setup_kubarr.py

  # Reset existing cluster
  python setup_kubarr.py --reset

  # Use existing cluster (skip cluster creation)
  python setup_kubarr.py --skip-cluster

  # Custom cluster name and port
  python setup_kubarr.py --cluster-name my-kubarr --port 9080

  # Skip image building (use existing images)
  python setup_kubarr.py --skip-build
        """
    )

    parser.add_argument(
        "--cluster-name", "-n",
        default="kubarr-test",
        help="Kind cluster name (default: kubarr-test)"
    )
    parser.add_argument(
        "--port", "-p",
        type=int,
        default=8080,
        help="Host port to expose dashboard (default: 8080)"
    )
    parser.add_argument(
        "--admin-email",
        default="admin@example.com",
        help="Admin user email (default: admin@example.com)"
    )
    parser.add_argument(
        "--admin-password",
        default="admin123",
        help="Admin user password (default: admin123)"
    )
    parser.add_argument(
        "--reset",
        action="store_true",
        help="Delete existing cluster and start fresh"
    )
    parser.add_argument(
        "--skip-cluster",
        action="store_true",
        help="Skip cluster creation (use existing)"
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip Docker image building"
    )
    parser.add_argument(
        "--skip-monitoring",
        action="store_true",
        help="Skip monitoring stack (Loki, Promtail, etc.)"
    )
    parser.add_argument(
        "--project-root",
        type=Path,
        default=None,
        help="Project root directory (auto-detected if not specified)"
    )

    args = parser.parse_args()

    # Detect project root
    if args.project_root:
        project_root = args.project_root
    else:
        # Try to find project root from script location
        script_path = Path(__file__).resolve()
        project_root = script_path.parent.parent

        # Verify it's the right directory
        if not (project_root / "charts").exists():
            log("Could not detect project root. Please specify --project-root", "error")
            sys.exit(1)

    print(f"{Colors.BOLD}Kubarr Setup Tool{Colors.END}")
    print(f"Project root: {project_root}")
    print()

    # Check prerequisites
    check_prerequisites()
    print()

    # Handle cluster
    if args.reset:
        delete_cluster(args.cluster_name)
        print()

    if not args.skip_cluster:
        create_cluster(args.cluster_name, args.port)
        print()

    # Build and load images
    if not args.skip_build:
        build_images(project_root)
        print()

    load_images(args.cluster_name)
    print()

    # Deploy Kubarr
    deploy_kubarr(project_root)
    print()

    # Deploy monitoring
    if not args.skip_monitoring:
        deploy_monitoring(project_root)
        print()

    # Complete setup
    setup_initial_admin(args.admin_email, args.admin_password)
    print()

    # Wait for oauth2-proxy
    wait_for_oauth2_proxy()
    print()

    # Expose service
    expose_service()

    # Print summary
    print_summary(args.admin_email, args.admin_password, args.port)


if __name__ == "__main__":
    main()
