# Kubarr

A Kubernetes-based automated media management stack for deploying and orchestrating popular media automation tools.

## Overview

Kubarr provides a complete Kubernetes deployment solution for running a fully automated media server stack, including:

- **Radarr** - Movie collection manager
- **Sonarr** - TV series collection manager
- **qBittorrent** - BitTorrent client with web interface
- **Jellyseerr** - Media request and discovery tool
- **Jellyfin** - Media server for streaming your content

## Features

- Python-based CLI tool for easy deployment and management
- Automated Kubernetes resource creation and configuration
- Persistent storage configuration for media and application data
- Service networking between components
- Scalable and reproducible infrastructure
- Simple command-line interface with rich terminal output

## Prerequisites

- Python 3.9 or higher
- [Poetry](https://python-poetry.org/docs/#installation) for dependency management
- Kubernetes cluster (v1.20+)
- `kubectl` configured to access your cluster
- Storage provisioner configured (for PersistentVolumes)
- Sufficient storage space for media files

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/kubarr.git
cd kubarr

# Install dependencies with Poetry
poetry install

# Activate the virtual environment
poetry shell
```

## Usage

### Deploy the Stack

```bash
# Deploy to default namespace (media)
kubarr deploy

# Deploy to custom namespace
kubarr deploy --namespace my-media

# Dry run to see what would be deployed
kubarr deploy --dry-run
```

### Check Status

```bash
# Check status of deployed services
kubarr status

# Check status in custom namespace
kubarr status --namespace my-media
```

### Remove the Stack

```bash
# Remove the entire stack
kubarr remove

# Remove from custom namespace
kubarr remove --namespace my-media
```

## Components

### Radarr
Automatically downloads movies via Usenet and BitTorrent. Monitors multiple RSS feeds and integrates with download clients.

### Sonarr
Manages TV show downloads, tracks new episodes, and automatically grabs, sorts, and renames them.

### qBittorrent
Feature-rich BitTorrent client with a web UI for managing downloads.

### Jellyseerr
Request management and media discovery tool that integrates with Radarr and Sonarr.

### Jellyfin
Free software media server for organizing, managing, and streaming media.

## Configuration

Configuration options will be added in future releases. Currently, the tool deploys with sensible defaults.

### Planned Configuration Options

- Storage sizes for each service
- Resource limits and requests
- Service exposure types (ClusterIP, NodePort, LoadBalancer)
- Ingress configuration
- Custom environment variables per service

## Accessing Services

After deployment, access the web interfaces:

```bash
# Port forward to access locally
kubectl port-forward -n media svc/radarr 7878:7878
kubectl port-forward -n media svc/sonarr 8989:8989
kubectl port-forward -n media svc/qbittorrent 8080:8080
kubectl port-forward -n media svc/jellyseerr 5055:5055
kubectl port-forward -n media svc/jellyfin 8096:8096
```

Then access via:
- Radarr: http://localhost:7878
- Sonarr: http://localhost:8989
- qBittorrent: http://localhost:8080
- Jellyseerr: http://localhost:5055
- Jellyfin: http://localhost:8096

## Development

### Running from Source

```bash
# Install development dependencies
poetry install --with dev

# Run the CLI directly
poetry run kubarr --help

# Run tests
poetry run pytest

# Format code
poetry run black .

# Lint code
poetry run ruff check .
```

### Project Structure

```
kubarr/
├── kubarr/           # Main package
│   ├── __init__.py   # Package initialization
│   ├── cli.py        # CLI commands
│   └── deploy.py     # Deployment logic
├── pyproject.toml    # Poetry configuration
└── README.md         # This file
```

## Backup

Ensure you regularly backup:
- Application configuration data
- Media metadata and databases
- Downloaded media files

## Monitoring

Consider integrating with:
- Prometheus for metrics
- Grafana for dashboards
- Loki for log aggregation

## Troubleshooting

### Pods not starting
```bash
kubectl describe pod <pod-name> -n media
kubectl logs <pod-name> -n media
```

### Storage issues
Check PersistentVolumeClaims:
```bash
kubectl get pvc -n media
```

### Network connectivity
Verify services are running:
```bash
kubectl get svc -n media
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## Disclaimer

This tool is provided for educational purposes. Users are responsible for ensuring their use complies with local laws and regulations. The authors do not condone piracy or copyright infringement. Use this software only with content you have the legal right to download and distribute.

## License

MIT License - See LICENSE file for details

## Support

For issues and questions:
- Open an issue on GitHub
- Check existing issues for solutions
- Consult individual application documentation

## Acknowledgments

Thanks to the developers of:
- [Radarr](https://radarr.video/)
- [Sonarr](https://sonarr.tv/)
- [qBittorrent](https://www.qbittorrent.org/)
- [Jellyseerr](https://github.com/Fallenbagel/jellyseerr)
- [Jellyfin](https://jellyfin.org/)
