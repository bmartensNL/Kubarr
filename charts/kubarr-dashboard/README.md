# Kubarr Dashboard Helm Chart

A Helm chart for deploying the Kubarr Dashboard - a Kubernetes-based media automation stack management interface.

## Prerequisites

- Kubernetes 1.20+
- Helm 3.0+
- Storage provisioner supporting PersistentVolumeClaims (for media apps)

## Installing the Chart

To install the chart with the release name `kubarr`:

```bash
helm install kubarr ./charts/kubarr-dashboard
```

Or with custom values:

```bash
helm install kubarr ./charts/kubarr-dashboard -f my-values.yaml
```

## Uninstalling the Chart

To uninstall/delete the `kubarr` deployment:

```bash
helm uninstall kubarr
```

## Configuration

The following table lists the configurable parameters of the Kubarr Dashboard chart and their default values.

### Global Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `namespace.create` | Create namespace | `true` |
| `namespace.name` | Namespace name | `kubarr-system` |

### Backend Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `backend.replicaCount` | Number of replicas | `1` |
| `backend.image.repository` | Backend image repository | `kubarr/dashboard-backend` |
| `backend.image.tag` | Backend image tag | `latest` |
| `backend.image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `backend.service.type` | Service type | `ClusterIP` |
| `backend.service.port` | Service port | `8000` |
| `backend.resources.limits.cpu` | CPU limit | `500m` |
| `backend.resources.limits.memory` | Memory limit | `512Mi` |

### Frontend Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `frontend.replicaCount` | Number of replicas | `1` |
| `frontend.image.repository` | Frontend image repository | `kubarr/dashboard-frontend` |
| `frontend.image.tag` | Frontend image tag | `latest` |
| `frontend.image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `frontend.service.type` | Service type | `ClusterIP` |
| `frontend.service.port` | Service port | `80` |

### RBAC Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `rbac.create` | Create RBAC resources | `true` |
| `serviceAccount.create` | Create service account | `true` |
| `serviceAccount.name` | Service account name | `kubarr-dashboard` |

### Ingress Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `ingress.enabled` | Enable ingress | `false` |
| `ingress.className` | Ingress class name | `nginx` |
| `ingress.hosts` | Ingress hosts | `[kubarr.local]` |

## Example Configurations

### With Ingress

```yaml
ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  hosts:
    - host: kubarr.example.com
      paths:
        - path: /
          pathType: Prefix
          backend: frontend
        - path: /api
          pathType: Prefix
          backend: backend
  tls:
    - secretName: kubarr-tls
      hosts:
        - kubarr.example.com
```

### With Custom Resources

```yaml
backend:
  resources:
    limits:
      cpu: 1000m
      memory: 1Gi
    requests:
      cpu: 200m
      memory: 512Mi

frontend:
  resources:
    limits:
      cpu: 500m
      memory: 512Mi
    requests:
      cpu: 100m
      memory: 256Mi
```

### With Custom Environment Variables

```yaml
backend:
  env:
    - name: KUBARR_IN_CLUSTER
      value: "true"
    - name: KUBARR_DEFAULT_NAMESPACE
      value: "my-media"
    - name: KUBARR_LOG_LEVEL
      value: "DEBUG"
```

## Accessing the Dashboard

### Port Forward (Development)

```bash
kubectl port-forward -n kubarr-system svc/kubarr-dashboard 8080:80
```

Then visit: http://localhost:8080

### Ingress (Production)

Configure ingress as shown above and access via your domain.

## Permissions

The dashboard requires the following Kubernetes permissions:

- **Read**: namespaces, pods, services, deployments, metrics
- **Write**: deployments, services, PVCs, ConfigMaps (for app installation)
- **Delete**: deployments, services, PVCs (for app removal)

These permissions are granted via a ClusterRole and ClusterRoleBinding.

## Upgrading

To upgrade to a new version:

```bash
helm upgrade kubarr ./charts/kubarr-dashboard
```

## Support

For issues and questions:
- GitHub: https://github.com/yourusername/kubarr/issues
- Documentation: https://github.com/yourusername/kubarr
