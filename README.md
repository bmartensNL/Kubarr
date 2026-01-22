# Kubarr

A Kubernetes-based automated media management stack for deploying and orchestrating popular media automation tools.

## Overview

Kubarr provides a complete Kubernetes deployment solution for running a fully automated media server stack, including:

- **Radarr** - Movie collection manager
- **Sonarr** - TV series collection manager
- **qBittorrent** - BitTorrent client with web interface
- **Jellyseerr** - Media request and discovery tool
- **Jellyfin** - Media server for streaming your content
- **Jackett** - Indexer proxy/aggregator
- **SABnzbd** - Usenet binary newsreader

The included web dashboard provides a modern interface for deploying, monitoring, and managing your media stack applications from your browser.

## Features

- **Web Dashboard** - Modern React-based UI for managing your stack
  - Browse and install apps from a catalog
  - Monitor pod status and resource usage in real-time
  - View logs from any pod
  - Start, stop, and delete applications
- **Python CLI** - Command-line tool for deployment and management
- **Automated Deployment** - Kubernetes resource creation and configuration
- **Persistent Storage** - Configuration for media and application data
- **Service Networking** - Automatic networking between components
- **Scalable Infrastructure** - Reproducible and maintainable setup
- **Rich Terminal Output** - Beautiful CLI with progress indicators

## Prerequisites

- Python 3.9 or higher
- [Poetry](https://python-poetry.org/docs/#installation) for dependency management
- Kubernetes cluster (v1.20+)
- `kubectl` configured to access your cluster
- [Helm](https://helm.sh/docs/intro/install/) v3 (for dashboard installation)
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

### Jackett
Proxy server that provides a single API for accessing multiple torrent indexers and trackers.

### SABnzbd
Usenet binary newsreader with a web interface for downloading from Usenet servers.

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

## Web Dashboard

Kubarr includes a modern web dashboard for managing your media stack through a browser interface. The dashboard runs inside your Kubernetes cluster and provides a centralized control panel for all your applications.

### Dashboard Features

- **App Catalog**: Browse all available applications (Radarr, Sonarr, qBittorrent, Jellyseerr, Jellyfin, Jackett, SABnzbd)
- **One-Click Installation**: Deploy apps with a single click
- **Real-Time Monitoring**: View pod status, health, and resource usage
- **Log Viewer**: Stream logs from any pod in real-time
- **App Management**: Start, stop, restart, and delete applications
- **Resource Metrics**: CPU and memory usage (requires metrics-server)

### Installing the Dashboard

The dashboard is deployed using Helm and runs inside your Kubernetes cluster with proper RBAC permissions.

```bash
# Install the dashboard to the default namespace (kubarr-system)
kubarr install-dashboard

# Install to a custom namespace
kubarr install-dashboard --namespace my-dashboard

# Customize values during installation
kubarr install-dashboard --set backend.image.tag=latest

# Use a custom kubeconfig
kubarr install-dashboard --kubeconfig /path/to/kubeconfig
```

The installation creates:
- A dedicated namespace (`kubarr-system` by default)
- Service account with RBAC permissions
- Backend deployment (FastAPI)
- Frontend deployment (React + Nginx)
- ClusterIP service

### Accessing the Dashboard

After installation, access the dashboard using port-forwarding:

```bash
# Port forward to localhost:8080
kubarr dashboard-port-forward

# Use a custom port
kubarr dashboard-port-forward --port 3000

# Port forward from a different namespace
kubarr dashboard-port-forward --namespace my-dashboard
```

Then open your browser to: http://localhost:8080

### Dashboard Architecture

The dashboard consists of:
- **Backend**: FastAPI server that communicates with the Kubernetes API
- **Frontend**: React application with Tailwind CSS
- **Authentication**: Uses Kubernetes service account (in-cluster)
- **RBAC**: ClusterRole with permissions to manage apps

### Advanced Configuration

You can customize the dashboard by modifying `charts/kubarr-dashboard/values.yaml`:

```yaml
# Backend configuration
backend:
  image:
    repository: kubarr/dashboard-backend
    tag: latest
  resources:
    requests:
      cpu: 100m
      memory: 256Mi

# Frontend configuration
frontend:
  image:
    repository: kubarr/dashboard-frontend
    tag: latest

# RBAC permissions
rbac:
  create: true
  rules:
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["get", "list", "create", "delete"]

# Ingress (optional)
ingress:
  enabled: false
  className: nginx
  hosts:
    - host: kubarr.local
      paths:
        - path: /
          pathType: Prefix
```

### Exposing with Ingress

For production access without port-forwarding, enable Ingress:

```bash
# Install with Ingress enabled
kubarr install-dashboard \
  --set ingress.enabled=true \
  --set ingress.hosts[0].host=kubarr.example.com
```

Then access at: https://kubarr.example.com

### Uninstalling the Dashboard

```bash
# Uninstall the dashboard
kubarr uninstall-dashboard

# Uninstall from a custom namespace
kubarr uninstall-dashboard --namespace my-dashboard
```

This removes all dashboard resources but preserves your deployed media apps.

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
├── kubarr/                      # Main package
│   ├── __init__.py              # Package initialization
│   ├── cli.py                   # CLI commands
│   ├── deploy.py                # Deployment logic
│   ├── core/                    # Core services
│   │   ├── models.py            # Pydantic models
│   │   ├── k8s_client.py        # Kubernetes client
│   │   ├── app_catalog.py       # App registry
│   │   ├── deployment_manager.py # Deployment logic
│   │   ├── monitoring_service.py # Monitoring
│   │   └── logs_service.py      # Log retrieval
│   └── api/                     # FastAPI backend
│       ├── main.py              # API entry point
│       ├── config.py            # Configuration
│       ├── dependencies.py      # DI container
│       ├── routers/             # API routes
│       │   ├── apps.py          # App management
│       │   ├── monitoring.py    # Monitoring
│       │   ├── logs.py          # Logs
│       │   └── system.py        # System info
│       └── websocket/           # WebSocket handlers
├── frontend/                    # React dashboard
│   ├── src/
│   │   ├── api/                 # API clients
│   │   ├── components/          # React components
│   │   ├── pages/               # Page components
│   │   ├── hooks/               # Custom hooks
│   │   └── types/               # TypeScript types
│   ├── package.json             # NPM dependencies
│   └── vite.config.ts           # Vite configuration
├── charts/                      # Helm charts
│   └── kubarr-dashboard/        # Dashboard chart
│       ├── Chart.yaml           # Chart metadata
│       ├── values.yaml          # Configuration
│       └── templates/           # K8s manifests
├── docker/                      # Docker files
│   ├── Dockerfile.backend       # Backend image
│   ├── Dockerfile.frontend      # Frontend image
│   └── nginx.conf               # Nginx config
├── tests/                       # Test suite
├── pyproject.toml               # Poetry configuration
└── README.md                    # This file
```

### Building Docker Images

To build and deploy custom dashboard images:

```bash
# Build backend image
docker build -f docker/Dockerfile.backend -t kubarr/dashboard-backend:latest .

# Build frontend image
docker build -f docker/Dockerfile.frontend -t kubarr/dashboard-frontend:latest .

# For local Kubernetes (kind/minikube), load images
kind load docker-image kubarr/dashboard-backend:latest
kind load docker-image kubarr/dashboard-frontend:latest

# Or push to a registry
docker push kubarr/dashboard-backend:latest
docker push kubarr/dashboard-frontend:latest

# Install dashboard with custom images
kubarr install-dashboard \
  --set backend.image.repository=your-registry/dashboard-backend \
  --set backend.image.tag=v1.0.0
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
