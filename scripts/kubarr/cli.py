"""CLI command definitions for Kubarr deployment tool."""

import argparse
import sys

from .config import (
    ALL_CHARTS,
    COMPONENTS,
    CORE_CHARTS,
    DEFAULT_ADMIN_EMAIL,
    DEFAULT_ADMIN_PASSWORD,
    DEFAULT_CLUSTER_NAME,
    DEFAULT_HOST_PORT,
    MONITORING_CHARTS,
    NAMESPACES,
    get_project_root,
)
from .utils import (
    Colors,
    check_prerequisites,
    confirm,
    die,
    log,
    log_header,
    log_subheader,
    run_quiet,
)


def cmd_setup(args: argparse.Namespace) -> int:
    """Full setup: create cluster, build images, deploy all charts."""
    from .cluster import create_cluster, delete_cluster, wait_for_cluster_ready
    from .docker import build_all, load_images_to_kind
    from .helm import deploy_all, expose_oauth2_proxy, setup_initial_admin

    log_header("Kubarr Setup")
    print(f"Project root: {get_project_root()}")
    print()

    # Check prerequisites
    if not check_prerequisites():
        die("Missing required tools")

    # Handle cluster
    if args.reset:
        log_header("Resetting Cluster")
        delete_cluster(args.cluster_name)

    if not args.skip_cluster:
        log_header("Creating Kind Cluster")
        if not create_cluster(args.cluster_name, args.port):
            die("Failed to create cluster")
        wait_for_cluster_ready(args.cluster_name)

    # Build images
    images = {}
    if not args.skip_build:
        log_header("Building Docker Images")
        images = build_all()
        if not images:
            die("Failed to build images")

        # Load images into Kind
        if not args.skip_cluster:
            log_header("Loading Images into Kind")
            if not load_images_to_kind(images, args.cluster_name):
                die("Failed to load images")

    # Deploy charts
    log_header("Deploying Charts")
    if not deploy_all(images, skip_monitoring=args.skip_monitoring):
        die("Failed to deploy charts")

    # Setup admin
    if not args.skip_setup:
        log_header("Initial Setup")
        setup_initial_admin(args.admin_email, args.admin_password)

    # Expose service
    if not args.skip_cluster:
        log_header("Exposing Service")
        expose_oauth2_proxy(node_port=30080)

    # Print summary
    print_summary(args.admin_email, args.admin_password, args.port)

    return 0


def cmd_reset(args: argparse.Namespace) -> int:
    """Delete cluster and optionally rebuild everything."""
    from .cluster import create_cluster, delete_cluster, wait_for_cluster_ready
    from .docker import build_all, load_images_to_kind
    from .helm import deploy_all, expose_oauth2_proxy, setup_initial_admin

    log_header("Resetting Kubarr")

    # Delete cluster
    delete_cluster(args.cluster_name)

    if args.full_rebuild:
        # Create new cluster
        log_header("Creating New Cluster")
        if not create_cluster(args.cluster_name, args.port):
            die("Failed to create cluster")
        wait_for_cluster_ready(args.cluster_name)

        # Build images
        log_header("Building Docker Images")
        images = build_all()
        if not images:
            die("Failed to build images")

        # Load images
        log_header("Loading Images into Kind")
        if not load_images_to_kind(images, args.cluster_name):
            die("Failed to load images")

        # Deploy charts
        log_header("Deploying Charts")
        if not deploy_all(images, skip_monitoring=args.skip_monitoring):
            die("Failed to deploy charts")

        # Setup admin
        log_header("Initial Setup")
        setup_initial_admin(args.admin_email, args.admin_password)

        # Expose service
        log_header("Exposing Service")
        expose_oauth2_proxy(node_port=30080)

        print_summary(args.admin_email, args.admin_password, args.port)

    return 0


def cmd_build(args: argparse.Namespace) -> int:
    """Build Docker images."""
    from .docker import build_all, build_image, get_timestamp_tag

    log_header("Building Docker Images")

    tag = args.tag or get_timestamp_tag()

    if args.component == "all":
        images = build_all(tag)
        if not images:
            die("Failed to build images")
        for component, image in images.items():
            print(f"  {component}: {image}")
    else:
        success, image = build_image(args.component, tag)
        if not success:
            die(f"Failed to build {args.component}")
        print(f"  {args.component}: {image}")

    return 0


def cmd_deploy(args: argparse.Namespace) -> int:
    """Deploy Helm charts."""
    from .helm import deploy_all, deploy_core, deploy_monitoring, install_chart

    log_header("Deploying Charts")

    if args.all:
        if not deploy_all(skip_monitoring=args.skip_monitoring):
            die("Failed to deploy charts")
    elif args.chart == "core":
        if not deploy_core(skip_image_override=True):
            die("Failed to deploy core charts")
    elif args.chart == "monitoring":
        if not deploy_monitoring():
            die("Failed to deploy monitoring charts")
    elif args.chart:
        if args.chart not in ALL_CHARTS:
            die(f"Unknown chart: {args.chart}. Available: {', '.join(ALL_CHARTS)}")

        extra_args = ""
        if args.values:
            from pathlib import Path
            values_path = Path(args.values)
            if not values_path.exists():
                die(f"Values file not found: {args.values}")
            extra_args = f"-f {values_path}"

        if not install_chart(args.chart, extra_args=extra_args):
            die(f"Failed to deploy {args.chart}")
    else:
        die("Specify --all or a chart name")

    log("Deployment complete", "success")
    return 0


def cmd_redeploy(args: argparse.Namespace) -> int:
    """Rebuild and redeploy a single component."""
    from .cluster import cluster_exists
    from .docker import build_image, get_timestamp_tag, load_image_to_kind, update_deployment_image

    if args.component not in COMPONENTS:
        die(f"Unknown component: {args.component}. Available: {', '.join(COMPONENTS)}")

    log_header(f"Redeploying {args.component}")

    # Build image
    tag = get_timestamp_tag()
    success, image = build_image(args.component, tag)
    if not success:
        die(f"Failed to build {args.component}")

    # Load into Kind if cluster exists
    if cluster_exists(args.cluster_name):
        log_subheader("Loading image into Kind...")
        if not load_image_to_kind(image, args.cluster_name):
            die("Failed to load image")

    # Update deployment
    log_subheader("Updating deployment...")
    if not update_deployment_image(args.component, image):
        die("Failed to update deployment")

    log(f"Redeployed {args.component}", "success")
    return 0


def cmd_status(args: argparse.Namespace) -> int:
    """Show cluster and deployment status."""
    from .cluster import get_cluster_info
    from .docker import get_current_images
    from .helm import get_chart_status

    log_header("Kubarr Status")

    # Cluster info
    log_subheader("Cluster")
    cluster_info = get_cluster_info(args.cluster_name)
    print(f"  Name: {cluster_info['name']}")
    print(f"  Exists: {'Yes' if cluster_info['exists'] else 'No'}")
    if cluster_info["exists"]:
        print(f"  Node Ready: {'Yes' if cluster_info.get('node_ready') else 'No'}")
        print(f"  Container: {'Running' if cluster_info.get('container_running') else 'Stopped'}")
    print()

    # Chart status
    log_subheader("Helm Releases")
    chart_status = get_chart_status()
    for chart, info in chart_status.items():
        status = "Installed" if info["installed"] else "Not installed"
        if info["installed"]:
            status += f" (v{info.get('version', '?')}, {info.get('status', 'unknown')})"
        print(f"  {chart}: {status}")
    print()

    # Pod status
    log_subheader("Pods")
    for namespace in NAMESPACES.values():
        success, output = run_quiet(
            f"kubectl get pods -n {namespace} --no-headers 2>/dev/null",
            check=False,
        )
        if success and output.strip():
            print(f"  [{namespace}]")
            for line in output.strip().split("\n"):
                print(f"    {line}")
    print()

    # Current images
    log_subheader("Current Images")
    images = get_current_images()
    for component, image in images.items():
        print(f"  {component}: {image}")

    return 0


def cmd_logs(args: argparse.Namespace) -> int:
    """Stream logs from a component."""
    from .config import DEPLOYMENTS

    if args.component not in COMPONENTS:
        die(f"Unknown component: {args.component}. Available: {', '.join(COMPONENTS)}")

    deployment = DEPLOYMENTS[args.component]
    namespace = NAMESPACES["kubarr"]

    log(f"Streaming logs from {args.component}...")

    cmd = f"kubectl logs -f deployment/{deployment} -n {namespace}"
    if args.previous:
        cmd += " --previous"
    if args.tail:
        cmd += f" --tail={args.tail}"

    import subprocess
    try:
        subprocess.run(cmd, shell=True)
    except KeyboardInterrupt:
        pass

    return 0


def print_summary(admin_email: str, admin_password: str, host_port: int) -> None:
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
    print("  - Kubarr Dashboard (backend + frontend)")
    print("  - nginx (reverse proxy)")
    print("  - OAuth2 Proxy (authentication)")
    print()
    print(f"{Colors.YELLOW}Note:{Colors.END} Access the dashboard at http://localhost:{host_port}")
    print(f"{'='*60}")


def main() -> int:
    """Main entry point for the CLI."""
    parser = argparse.ArgumentParser(
        description="Kubarr Deployment Tool",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Full setup with new cluster
  python kubarr.py setup

  # Reset and rebuild everything
  python kubarr.py reset --full-rebuild

  # Rebuild and redeploy just the frontend
  python kubarr.py redeploy frontend

  # Deploy charts only (no rebuild)
  python kubarr.py deploy --all

  # Check status
  python kubarr.py status
        """,
    )

    # Global options
    parser.add_argument(
        "--cluster-name", "-n",
        default=DEFAULT_CLUSTER_NAME,
        help=f"Kind cluster name (default: {DEFAULT_CLUSTER_NAME})",
    )

    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # setup command
    setup_parser = subparsers.add_parser("setup", help="Full setup")
    setup_parser.add_argument(
        "--port", "-p",
        type=int,
        default=DEFAULT_HOST_PORT,
        help=f"Host port (default: {DEFAULT_HOST_PORT})",
    )
    setup_parser.add_argument(
        "--admin-email",
        default=DEFAULT_ADMIN_EMAIL,
        help=f"Admin email (default: {DEFAULT_ADMIN_EMAIL})",
    )
    setup_parser.add_argument(
        "--admin-password",
        default=DEFAULT_ADMIN_PASSWORD,
        help=f"Admin password (default: {DEFAULT_ADMIN_PASSWORD})",
    )
    setup_parser.add_argument(
        "--reset",
        action="store_true",
        help="Delete existing cluster first",
    )
    setup_parser.add_argument(
        "--skip-cluster",
        action="store_true",
        help="Skip cluster creation (use existing)",
    )
    setup_parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip Docker image building",
    )
    setup_parser.add_argument(
        "--skip-monitoring",
        action="store_true",
        help="Skip monitoring stack",
    )
    setup_parser.add_argument(
        "--skip-setup",
        action="store_true",
        help="Skip initial admin setup",
    )
    setup_parser.set_defaults(func=cmd_setup)

    # reset command
    reset_parser = subparsers.add_parser("reset", help="Delete cluster and start fresh")
    reset_parser.add_argument(
        "--full-rebuild",
        action="store_true",
        help="Rebuild everything after reset",
    )
    reset_parser.add_argument(
        "--port", "-p",
        type=int,
        default=DEFAULT_HOST_PORT,
        help=f"Host port (default: {DEFAULT_HOST_PORT})",
    )
    reset_parser.add_argument(
        "--admin-email",
        default=DEFAULT_ADMIN_EMAIL,
        help=f"Admin email (default: {DEFAULT_ADMIN_EMAIL})",
    )
    reset_parser.add_argument(
        "--admin-password",
        default=DEFAULT_ADMIN_PASSWORD,
        help=f"Admin password (default: {DEFAULT_ADMIN_PASSWORD})",
    )
    reset_parser.add_argument(
        "--skip-monitoring",
        action="store_true",
        help="Skip monitoring stack",
    )
    reset_parser.set_defaults(func=cmd_reset)

    # build command
    build_parser = subparsers.add_parser("build", help="Build Docker images")
    build_parser.add_argument(
        "component",
        nargs="?",
        default="all",
        choices=COMPONENTS + ["all"],
        help="Component to build (default: all)",
    )
    build_parser.add_argument(
        "--tag", "-t",
        help="Image tag (default: timestamp)",
    )
    build_parser.set_defaults(func=cmd_build)

    # deploy command
    deploy_parser = subparsers.add_parser("deploy", help="Deploy Helm charts")
    deploy_parser.add_argument(
        "chart",
        nargs="?",
        help=f"Chart to deploy ({', '.join(ALL_CHARTS)}, core, monitoring)",
    )
    deploy_parser.add_argument(
        "--all", "-a",
        action="store_true",
        help="Deploy all charts",
    )
    deploy_parser.add_argument(
        "--skip-monitoring",
        action="store_true",
        help="Skip monitoring charts",
    )
    deploy_parser.add_argument(
        "--values", "-f",
        help="Custom values file",
    )
    deploy_parser.set_defaults(func=cmd_deploy)

    # redeploy command
    redeploy_parser = subparsers.add_parser("redeploy", help="Rebuild and redeploy a component")
    redeploy_parser.add_argument(
        "component",
        choices=COMPONENTS,
        help="Component to redeploy",
    )
    redeploy_parser.set_defaults(func=cmd_redeploy)

    # status command
    status_parser = subparsers.add_parser("status", help="Show status")
    status_parser.set_defaults(func=cmd_status)

    # logs command
    logs_parser = subparsers.add_parser("logs", help="Stream component logs")
    logs_parser.add_argument(
        "component",
        choices=COMPONENTS,
        help="Component to get logs from",
    )
    logs_parser.add_argument(
        "--previous", "-p",
        action="store_true",
        help="Show previous container logs",
    )
    logs_parser.add_argument(
        "--tail", "-t",
        type=int,
        help="Number of lines to show",
    )
    logs_parser.set_defaults(func=cmd_logs)

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return 1

    return args.func(args)
