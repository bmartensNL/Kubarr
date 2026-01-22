"""Deployment logic for Kubarr."""

from kubernetes import client, config
from kubernetes.client.rest import ApiException
from rich.console import Console
from rich.table import Table

console = Console()


def deploy_stack(namespace: str, kubeconfig: str | None = None, dry_run: bool = False) -> None:
    """Deploy the complete media stack to Kubernetes.

    Args:
        namespace: Kubernetes namespace to deploy to
        kubeconfig: Path to kubeconfig file (optional)
        dry_run: If True, only show what would be deployed
    """
    # Load Kubernetes configuration
    if kubeconfig:
        config.load_kube_config(config_file=kubeconfig)
    else:
        try:
            config.load_incluster_config()
        except config.ConfigException:
            config.load_kube_config()

    v1 = client.CoreV1Api()

    # Create namespace if it doesn't exist
    if not dry_run:
        try:
            v1.create_namespace(
                client.V1Namespace(metadata=client.V1ObjectMeta(name=namespace))
            )
            console.print(f"[green]Created namespace: {namespace}[/green]")
        except ApiException as e:
            if e.status == 409:  # Already exists
                console.print(f"[dim]Namespace {namespace} already exists[/dim]")
            else:
                raise
    else:
        console.print(f"[dim]Would create namespace: {namespace}[/dim]")

    # TODO: Deploy actual services
    # This is where you'll add the deployment logic for:
    # - Radarr
    # - Sonarr
    # - qBittorrent
    # - Jellyseerr
    # - Jellyfin
    # - Jackett
    # - SABnzbd
    # etc.

    console.print("[yellow]Note: Service deployments not yet implemented[/yellow]")
    console.print("[dim]This will be implemented in future commits[/dim]")


def remove_stack(namespace: str, kubeconfig: str | None = None) -> None:
    """Remove the complete media stack from Kubernetes.

    Args:
        namespace: Kubernetes namespace to remove from
        kubeconfig: Path to kubeconfig file (optional)
    """
    # Load Kubernetes configuration
    if kubeconfig:
        config.load_kube_config(config_file=kubeconfig)
    else:
        try:
            config.load_incluster_config()
        except config.ConfigException:
            config.load_kube_config()

    v1 = client.CoreV1Api()

    try:
        v1.delete_namespace(name=namespace)
        console.print(f"[green]Deleted namespace: {namespace}[/green]")
    except ApiException as e:
        if e.status == 404:
            console.print(f"[yellow]Namespace {namespace} not found[/yellow]")
        else:
            raise


def check_status(namespace: str, kubeconfig: str | None = None) -> None:
    """Check the status of deployed services.

    Args:
        namespace: Kubernetes namespace to check
        kubeconfig: Path to kubeconfig file (optional)
    """
    # Load Kubernetes configuration
    if kubeconfig:
        config.load_kube_config(config_file=kubeconfig)
    else:
        try:
            config.load_incluster_config()
        except config.ConfigException:
            config.load_kube_config()

    v1 = client.CoreV1Api()

    try:
        # Get pods in namespace
        pods = v1.list_namespaced_pod(namespace=namespace)

        if not pods.items:
            console.print(f"[yellow]No pods found in namespace {namespace}[/yellow]")
            return

        # Create status table
        table = Table(title=f"Kubarr Stack Status - {namespace}")
        table.add_column("Service", style="cyan")
        table.add_column("Status", style="magenta")
        table.add_column("Restarts", style="yellow")
        table.add_column("Age", style="green")

        for pod in pods.items:
            name = pod.metadata.name
            status = pod.status.phase
            restarts = sum(
                cs.restart_count for cs in (pod.status.container_statuses or [])
            )
            age = pod.metadata.creation_timestamp

            table.add_row(name, status, str(restarts), str(age))

        console.print(table)

    except ApiException as e:
        if e.status == 404:
            console.print(f"[yellow]Namespace {namespace} not found[/yellow]")
        else:
            raise
