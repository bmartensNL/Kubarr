"""CLI interface for Kubarr deployment tool."""

import os
import subprocess
from pathlib import Path

import click
from rich.console import Console
from rich.panel import Panel
from rich.table import Table

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


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="kubarr-system",
    help="Kubernetes namespace for the dashboard",
    show_default=True,
)
@click.option(
    "--release-name",
    "-r",
    default="kubarr-dashboard",
    help="Helm release name",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
@click.option(
    "--enable-oauth2",
    is_flag=True,
    help="Enable OAuth2 authentication for media apps",
)
@click.option(
    "--domain",
    default=None,
    help="Domain name for the dashboard (required for OAuth2)",
)
@click.option(
    "--enable-ingress",
    is_flag=True,
    help="Enable ingress (required for OAuth2)",
)
@click.option(
    "--enable-tls",
    is_flag=True,
    help="Enable TLS/HTTPS",
)
@click.option(
    "--tls-secret-name",
    default="kubarr-tls",
    help="Name of TLS secret",
)
@click.option(
    "--set",
    "-s",
    multiple=True,
    help="Set values on the command line (can specify multiple)",
)
def install_dashboard(
    namespace: str,
    release_name: str,
    kubeconfig: str | None,
    enable_oauth2: bool,
    domain: str | None,
    enable_ingress: bool,
    enable_tls: bool,
    tls_secret_name: str,
    set: tuple[str, ...]
) -> None:
    """Install the Kubarr dashboard using Helm."""
    console.print(
        Panel.fit(
            "[bold cyan]Installing Kubarr Dashboard[/bold cyan]",
            border_style="cyan",
        )
    )

    # Validate OAuth2 requirements
    if enable_oauth2:
        if not domain:
            console.print("[bold red]✗[/bold red] --domain is required when --enable-oauth2 is set")
            raise click.Abort()
        if not enable_ingress:
            console.print("[yellow]⚠[/yellow]  OAuth2 requires ingress, auto-enabling...")
            enable_ingress = True

    # Get the charts directory
    chart_path = Path(__file__).parent.parent / "charts" / "kubarr-dashboard"

    if not chart_path.exists():
        console.print(f"[bold red]✗[/bold red] Chart not found at {chart_path}")
        raise click.Abort()

    # Build helm command
    helm_cmd = [
        "helm", "install", release_name, str(chart_path),
        "--namespace", namespace,
        "--create-namespace",
    ]

    if kubeconfig:
        helm_cmd.extend(["--kubeconfig", kubeconfig])

    # Add OAuth2 configuration
    if enable_oauth2:
        helm_cmd.extend(["--set", "oauth2.enabled=true"])
        if domain:
            helm_cmd.extend(["--set", f"ingress.hosts[0].host={domain}"])
            helm_cmd.extend(["--set", f"oauth2.proxy.config.redirectUrl=https://{domain}/oauth2/callback"])

    # Add ingress configuration
    if enable_ingress:
        helm_cmd.extend(["--set", "ingress.enabled=true"])
        if domain:
            helm_cmd.extend(["--set", f"ingress.hosts[0].host={domain}"])

    # Add TLS configuration
    if enable_tls:
        helm_cmd.extend(["--set", "ingress.tls.enabled=true"])
        if domain and tls_secret_name:
            helm_cmd.extend(["--set", f"ingress.tls[0].secretName={tls_secret_name}"])
            helm_cmd.extend(["--set", f"ingress.tls[0].hosts[0]={domain}"])

    # Add custom values
    for value in set:
        helm_cmd.extend(["--set", value])

    try:
        console.print(f"\n[dim]Running: {' '.join(helm_cmd)}[/dim]\n")
        result = subprocess.run(
            helm_cmd,
            check=True,
            capture_output=True,
            text=True,
        )
        console.print(result.stdout)

        console.print("\n[bold green]✓[/bold green] Dashboard installed successfully!")

        # Show access instructions
        table = Table(title="Access the Dashboard", show_header=False)
        table.add_row(
            "[cyan]Port Forward:[/cyan]",
            f"kubarr dashboard-port-forward -n {namespace}",
        )
        table.add_row(
            "[cyan]Or manually:[/cyan]",
            f"kubectl port-forward -n {namespace} svc/{release_name} 8080:80",
        )
        table.add_row(
            "[cyan]Then visit:[/cyan]",
            "http://localhost:8080",
        )
        console.print(table)

    except subprocess.CalledProcessError as e:
        console.print(f"[bold red]✗[/bold red] Installation failed: {e.stderr}")
        raise click.Abort()
    except FileNotFoundError:
        console.print("[bold red]✗[/bold red] Helm not found. Please install Helm first.")
        console.print("Visit: https://helm.sh/docs/intro/install/")
        raise click.Abort()


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="kubarr-system",
    help="Kubernetes namespace of the dashboard",
    show_default=True,
)
@click.option(
    "--release-name",
    "-r",
    default="kubarr-dashboard",
    help="Helm release name",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
@click.confirmation_option(prompt="Are you sure you want to uninstall the dashboard?")
def uninstall_dashboard(
    namespace: str, release_name: str, kubeconfig: str | None
) -> None:
    """Uninstall the Kubarr dashboard."""
    console.print("[yellow]Uninstalling Kubarr dashboard...[/yellow]")

    helm_cmd = ["helm", "uninstall", release_name, "--namespace", namespace]

    if kubeconfig:
        helm_cmd.extend(["--kubeconfig", kubeconfig])

    try:
        result = subprocess.run(
            helm_cmd,
            check=True,
            capture_output=True,
            text=True,
        )
        console.print(result.stdout)
        console.print("[bold green]✓[/bold green] Dashboard uninstalled successfully!")
    except subprocess.CalledProcessError as e:
        console.print(f"[bold red]✗[/bold red] Uninstallation failed: {e.stderr}")
        raise click.Abort()
    except FileNotFoundError:
        console.print("[bold red]✗[/bold red] Helm not found. Please install Helm first.")
        raise click.Abort()


@main.command()
@click.option(
    "--namespace",
    "-n",
    default="kubarr-system",
    help="Kubernetes namespace of the dashboard",
    show_default=True,
)
@click.option(
    "--release-name",
    "-r",
    default="kubarr-dashboard",
    help="Helm release name",
    show_default=True,
)
@click.option(
    "--port",
    "-p",
    default=8080,
    help="Local port to forward to",
    show_default=True,
)
@click.option(
    "--kubeconfig",
    "-k",
    default=None,
    help="Path to kubeconfig file (uses default if not specified)",
)
def dashboard_port_forward(
    namespace: str, release_name: str, port: int, kubeconfig: str | None
) -> None:
    """Port-forward to access the dashboard locally."""
    console.print(
        f"[cyan]Port-forwarding dashboard to localhost:{port}[/cyan]"
    )
    console.print(f"[dim]Press Ctrl+C to stop[/dim]\n")

    kubectl_cmd = [
        "kubectl", "port-forward",
        "-n", namespace,
        f"svc/{release_name}",
        f"{port}:80",
    ]

    if kubeconfig:
        kubectl_cmd.extend(["--kubeconfig", kubeconfig])

    try:
        console.print(f"[green]Dashboard available at:[/green] http://localhost:{port}\n")
        subprocess.run(kubectl_cmd, check=True)
    except subprocess.CalledProcessError as e:
        console.print(f"[bold red]✗[/bold red] Port-forward failed")
        raise click.Abort()
    except FileNotFoundError:
        console.print("[bold red]✗[/bold red] kubectl not found. Please install kubectl first.")
        raise click.Abort()
    except KeyboardInterrupt:
        console.print("\n[yellow]Port-forward stopped[/yellow]")


if __name__ == "__main__":
    main()
