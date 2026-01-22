"""CLI interface for Kubarr deployment tool."""

import click
from rich.console import Console
from rich.panel import Panel

from kubarr import __version__
from kubarr.deploy import deploy_stack, remove_stack, check_status

console = Console()


@click.group()
@click.version_option(version=__version__)
def main() -> None:
    """Kubarr - Deploy a complete media automation stack on Kubernetes."""
    pass


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="media",
    help="Kubernetes namespace to deploy to",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
@click.option(
    "--dry-run",
    is_flag=True,
    help="Show what would be deployed without actually deploying",
)
def deploy(namespace: str, kubeconfig: str | None, dry_run: bool) -> None:
    """Deploy the Kubarr media stack to Kubernetes."""
    console.print(
        Panel.fit(
            f"[bold cyan]Deploying Kubarr v{__version__}[/bold cyan]",
            border_style="cyan",
        )
    )

    try:
        deploy_stack(namespace=namespace, kubeconfig=kubeconfig, dry_run=dry_run)

        if not dry_run:
            console.print("\n[bold green]✓[/bold green] Deployment successful!")
            console.print(f"\nAccess your services with:\n")
            console.print(f"  kubectl port-forward -n {namespace} svc/radarr 7878:7878")
            console.print(f"  kubectl port-forward -n {namespace} svc/sonarr 8989:8989")
            console.print(f"  kubectl port-forward -n {namespace} svc/qbittorrent 8080:8080")
            console.print(f"  kubectl port-forward -n {namespace} svc/jellyseerr 5055:5055")
            console.print(f"  kubectl port-forward -n {namespace} svc/jellyfin 8096:8096")
    except Exception as e:
        console.print(f"[bold red]✗[/bold red] Deployment failed: {e}")
        raise click.Abort()


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="media",
    help="Kubernetes namespace to remove from",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
@click.confirmation_option(prompt="Are you sure you want to remove the entire stack?")
def remove(namespace: str, kubeconfig: str | None) -> None:
    """Remove the Kubarr media stack from Kubernetes."""
    console.print("[yellow]Removing Kubarr stack...[/yellow]")

    try:
        remove_stack(namespace=namespace, kubeconfig=kubeconfig)
        console.print("[bold green]✓[/bold green] Stack removed successfully!")
    except Exception as e:
        console.print(f"[bold red]✗[/bold red] Removal failed: {e}")
        raise click.Abort()


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="media",
    help="Kubernetes namespace to check",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
def status(namespace: str, kubeconfig: str | None) -> None:
    """Check the status of deployed services."""
    try:
        check_status(namespace=namespace, kubeconfig=kubeconfig)
    except Exception as e:
        console.print(f"[bold red]✗[/bold red] Status check failed: {e}")
        raise click.Abort()


if __name__ == "__main__":
    main()
