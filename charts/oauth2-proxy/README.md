# Kubarr OAuth2 Proxy Helm Chart

This Helm chart deploys [oauth2-proxy](https://github.com/oauth2-proxy/oauth2-proxy) to protect your Kubarr media applications with OAuth2 authentication.

## Overview

The OAuth2 Proxy sits in front of your media applications (Radarr, Sonarr, Jellyfin, etc.) and requires users to authenticate via the Kubarr dashboard OAuth2 provider before accessing them.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- Kubarr Dashboard with OAuth2 provider enabled

## Installation

### 1. Install the Kubarr Dashboard (OAuth2 Provider)

First, ensure the Kubarr dashboard is installed with OAuth2 enabled:

```bash
helm install kubarr ../kubarr \
  --namespace kubarr \
  --create-namespace \
  --set oauth2.enabled=true
```

### 2. Get OAuth2 Client Credentials

After the dashboard setup is complete, retrieve the OAuth2 client credentials from the setup response or database.

### 3. Install OAuth2 Proxy

Install the OAuth2 proxy with the client credentials:

```bash
helm install oauth2-proxy . \
  --namespace oauth2-proxy \
  --create-namespace \
  --set config.clientSecret="your-client-secret" \
  --set config.cookieSecret="your-cookie-secret" \
  --set config.redirectUrl="https://your-domain.com/oauth2/callback"
```

## Configuration

### Key Values

| Parameter | Description | Default |
|-----------|-------------|---------|
| `namespace.name` | Namespace name | `oauth2-proxy` |
| `replicaCount` | Number of replicas | `1` |
| `image.repository` | OAuth2 proxy image | `quay.io/oauth2-proxy/oauth2-proxy` |
| `image.tag` | Image tag | `v7.5.1` |
| `provider.issuerUrl` | OIDC issuer URL (dashboard) | `http://kubarr.kubarr.svc.cluster.local:8000/auth` |
| `config.clientId` | OAuth2 client ID | `oauth2-proxy` |
| `config.clientSecret` | OAuth2 client secret (required) | `""` |
| `config.cookieSecret` | Cookie encryption secret (required) | `""` |
| `config.redirectUrl` | OAuth2 redirect URL | `http://localhost:8080/oauth2/callback` |
| `config.upstreams` | Upstream services to proxy | `["http://kubarr:80"]` |
| `service.port` | Service port | `4180` |

### Generate Secrets

Generate a secure cookie secret:

```bash
python3 -c 'import secrets; print(secrets.token_hex(16))'
```

## Usage

### With Ingress

Configure your ingress to route protected paths through the OAuth2 proxy:

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: kubarr-apps
  annotations:
    nginx.ingress.kubernetes.io/auth-url: "http://oauth2-proxy.oauth2-proxy.svc.cluster.local:4180/oauth2/auth"
    nginx.ingress.kubernetes.io/auth-signin: "https://$host/auth/login?rd=$escaped_request_uri"
spec:
  rules:
    - host: your-domain.com
      http:
        paths:
          - path: /apps
            pathType: Prefix
            backend:
              service:
                name: oauth2-proxy
                port:
                  number: 4180
```

### Direct Access

Access the OAuth2 proxy directly:

```bash
kubectl port-forward -n oauth2-proxy svc/oauth2-proxy 4180:4180
```

Then navigate to `http://localhost:4180` - you'll be redirected to the login page.

## Uninstallation

```bash
helm uninstall oauth2-proxy -n oauth2-proxy
```

## Troubleshooting

### OAuth2 proxy not starting

Check logs:
```bash
kubectl logs -n oauth2-proxy -l app=oauth2-proxy
```

### OIDC Discovery failing

Ensure the dashboard OAuth2 provider is accessible:
```bash
kubectl exec -n oauth2-proxy deployment/oauth2-proxy -- \
  wget -q -O - http://kubarr.kubarr.svc.cluster.local:8000/auth/.well-known/openid-configuration
```

### Authentication not working

Verify the OAuth2 client credentials match what was created during dashboard setup.

## Resources

- [OAuth2 Proxy Documentation](https://oauth2-proxy.github.io/oauth2-proxy/)
- [Kubarr Documentation](https://github.com/yourusername/kubarr)
