# Kubarr Helm Charts

This directory contains Helm charts for deploying Kubarr and its components.

## Available Charts

### 1. kubarr
The main Kubarr dashboard application with OAuth2 provider capability.

**Components:**
- Rust/Axum backend with OAuth2 authorization server
- React frontend UI
- SQLite database for user management
- RBAC and ServiceAccount configuration

**Features:**
- User management with approval workflow
- OAuth2/OIDC provider (authorization code flow with PKCE)
- JWT RS256 token signing
- Self-registration support
- Media app deployment management

**Installation:**
```bash
helm install kubarr ./kubarr \
  --namespace kubarr \
  --create-namespace \
  --set oauth2.enabled=true
```

See [kubarr/README.md](kubarr/README.md) for details.

### 2. oauth2-proxy
OAuth2 reverse proxy for protecting media applications.

**Components:**
- oauth2-proxy deployment
- Service and configuration
- Secret management for credentials

**Features:**
- OIDC discovery and authentication
- Cookie-based session management
- Request proxying to upstream services
- Header forwarding for authentication

**Installation:**
```bash
helm install oauth2-proxy ./oauth2-proxy \
  --namespace oauth2-proxy \
  --create-namespace \
  --set config.clientSecret="your-client-secret" \
  --set config.cookieSecret="your-cookie-secret"
```

See [oauth2-proxy/README.md](oauth2-proxy/README.md) for details.

## Architecture

```
┌─────────────────────────────────────────────────┐
│           Ingress Controller (nginx)             │
└──────────────┬──────────────────────────────────┘
               │
               ├─> /auth/*       → kubarr (OAuth2 Provider)
               ├─> /oauth2/*     → oauth2-proxy (Callback)
               ├─> /dashboard/*  → kubarr (UI/API)
               └─> /apps/*       → oauth2-proxy → Media Apps
                                        │
                                        ├─> Radarr
                                        ├─> Sonarr
                                        ├─> Jellyfin
                                        └─> etc.
```

## Quick Start

### 1. Install Dashboard with OAuth2
```bash
# Generate JWT keys
cd scripts
./generate-jwt-keys.sh ../secrets
kubectl apply -f ../secrets/jwt-keys-secret.yaml

# Install dashboard
helm install kubarr ./charts/kubarr \
  --namespace kubarr \
  --create-namespace \
  --set oauth2.enabled=true \
  --set auth.jwt.existingSecret=kubarr-jwt-keys
```

### 2. Initialize Setup
```bash
# Port forward
kubectl port-forward -n kubarr svc/kubarr 8080:80

# Run setup
curl -X POST http://localhost:8080/api/setup/initialize \
  -H "Content-Type: application/json" \
  -d '{
    "admin_username": "admin",
    "admin_email": "admin@example.com",
    "admin_password": "your-password",
    "base_url": "http://localhost:8080"
  }'
```

The response will include the OAuth2 client credentials.

### 3. Install OAuth2 Proxy
```bash
helm install oauth2-proxy ./charts/oauth2-proxy \
  --namespace oauth2-proxy \
  --create-namespace \
  --set config.clientSecret="<client-secret-from-setup>" \
  --set config.cookieSecret="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
```

### 4. Deploy Media Apps
Use the Kubarr CLI to deploy media applications:
```bash
kubarr deploy radarr sonarr jellyfin
```

## Development

### Testing Charts Locally

#### Dashboard
```bash
helm install test-dashboard ./kubarr \
  --namespace test \
  --create-namespace \
  --dry-run --debug
```

#### OAuth2 Proxy
```bash
helm install test-oauth2 ./oauth2-proxy \
  --namespace test \
  --set config.clientSecret="test" \
  --set config.cookieSecret="test" \
  --dry-run --debug
```

### Linting
```bash
helm lint ./kubarr
helm lint ./oauth2-proxy
```

## Chart Dependencies

The **oauth2-proxy** chart requires the **kubarr** chart to be deployed first (with OAuth2 enabled) as it provides the OAuth2 authorization server.

## Upgrading

### Dashboard
```bash
helm upgrade kubarr ./kubarr \
  --namespace kubarr \
  --reuse-values
```

### OAuth2 Proxy
```bash
helm upgrade oauth2-proxy ./oauth2-proxy \
  --namespace oauth2-proxy \
  --reuse-values
```

## Uninstallation

```bash
# Remove OAuth2 proxy first
helm uninstall oauth2-proxy -n oauth2-proxy

# Then remove dashboard
helm uninstall kubarr -n kubarr
```

## Support

For issues and questions:
- GitHub Issues: https://github.com/yourusername/kubarr/issues
- Documentation: https://github.com/yourusername/kubarr/docs
