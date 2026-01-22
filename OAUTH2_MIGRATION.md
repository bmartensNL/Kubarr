# OAuth2 Chart Migration Guide

This guide helps you migrate from the monolithic OAuth2 configuration (where oauth2-proxy was part of kubarr-dashboard) to the new modular chart structure.

## What Changed?

The OAuth2 proxy has been moved to its own Helm chart for better modularity and easier management.

**Before:**
```
charts/
└── kubarr-dashboard/
    ├── templates/
    │   ├── oauth2-proxy-deployment.yaml
    │   ├── oauth2-proxy-service.yaml
    │   ├── oauth2-proxy-configmap.yaml
    │   └── oauth2-proxy-secret.yaml
    └── values.yaml (contains oauth2.proxy config)
```

**After:**
```
charts/
├── kubarr-dashboard/          # OAuth2 provider only
│   ├── templates/
│   └── values.yaml (simplified oauth2 config)
└── kubarr-oauth2-proxy/       # OAuth2 proxy as separate chart
    ├── templates/
    │   ├── deployment.yaml
    │   ├── service.yaml
    │   ├── configmap.yaml
    │   └── secret.yaml
    └── values.yaml
```

## Migration Steps

### Option 1: Fresh Installation (Recommended)

If you're starting fresh or can afford downtime:

1. **Uninstall existing deployment:**
   ```bash
   helm uninstall kubarr-dashboard -n kubarr-system
   ```

2. **Reinstall with new chart structure:**
   ```bash
   # Install dashboard
   helm install kubarr-dashboard ./charts/kubarr-dashboard \
     --namespace kubarr-system \
     --set oauth2.enabled=true

   # Install OAuth2 proxy separately
   helm install kubarr-oauth2-proxy ./charts/kubarr-oauth2-proxy \
     --namespace kubarr-system \
     --set config.clientSecret="your-secret" \
     --set config.cookieSecret="your-cookie-secret"
   ```

### Option 2: Zero-Downtime Migration

If you need to avoid downtime:

1. **Extract current OAuth2 proxy configuration:**
   ```bash
   # Get current client secret
   kubectl get secret kubarr-dashboard-oauth2-proxy-secret \
     -n kubarr-system \
     -o jsonpath='{.data.client-secret}' | base64 -d

   # Get current cookie secret
   kubectl get secret kubarr-dashboard-oauth2-proxy-secret \
     -n kubarr-system \
     -o jsonpath='{.data.cookie-secret}' | base64 -d

   # Get current redirect URL from configmap
   kubectl get configmap kubarr-dashboard-oauth2-proxy-config \
     -n kubarr-system \
     -o yaml
   ```

2. **Install new OAuth2 proxy chart:**
   ```bash
   helm install kubarr-oauth2-proxy ./charts/kubarr-oauth2-proxy \
     --namespace kubarr-system \
     --set config.clientSecret="<client-secret-from-step-1>" \
     --set config.cookieSecret="<cookie-secret-from-step-1>" \
     --set config.redirectUrl="<redirect-url-from-step-1>"
   ```

3. **Wait for new proxy to be ready:**
   ```bash
   kubectl wait --for=condition=ready pod \
     -l app=oauth2-proxy \
     -n kubarr-system \
     --timeout=60s
   ```

4. **Upgrade dashboard to remove old proxy:**
   ```bash
   helm upgrade kubarr-dashboard ./charts/kubarr-dashboard \
     --namespace kubarr-system \
     --reuse-values
   ```

   The old OAuth2 proxy resources will be automatically removed since they're no longer in the templates.

5. **Clean up old resources (if any remain):**
   ```bash
   kubectl delete deployment kubarr-dashboard-oauth2-proxy -n kubarr-system
   kubectl delete service kubarr-dashboard-oauth2-proxy -n kubarr-system
   kubectl delete configmap kubarr-dashboard-oauth2-proxy-config -n kubarr-system
   kubectl delete secret kubarr-dashboard-oauth2-proxy-secret -n kubarr-system
   ```

## Configuration Changes

### Dashboard values.yaml

**Old structure:**
```yaml
oauth2:
  enabled: true
  provider:
    issuerUrl: "http://kubarr-dashboard:8000"
  proxy:
    namespace: "media"
    replicaCount: 1
    image:
      repository: quay.io/oauth2-proxy/oauth2-proxy
      tag: "v7.5.1"
    config:
      clientId: "oauth2-proxy"
      clientSecret: "xxx"
      cookieSecret: "xxx"
      # ... more proxy config
```

**New structure:**
```yaml
oauth2:
  enabled: true
  provider:
    issuerUrl: "http://kubarr-dashboard.kubarr-system.svc.cluster.local:8000"
  # proxy config moved to separate chart
```

### New OAuth2 Proxy values.yaml

Create a new `oauth2-values.yaml`:
```yaml
namespace: kubarr-system

image:
  repository: quay.io/oauth2-proxy/oauth2-proxy
  tag: "v7.5.1"

provider:
  issuerUrl: "http://kubarr-dashboard.kubarr-system.svc.cluster.local:8000/auth"

config:
  clientId: "oauth2-proxy"
  clientSecret: "your-client-secret"
  cookieSecret: "your-cookie-secret"
  redirectUrl: "http://localhost:8080/oauth2/callback"
  upstreams:
    - "http://kubarr-dashboard:80"
```

## Ingress Updates

If you're using the unified ingress (ingress-unified.yaml), update it to point to the new service name:

**Old:**
```yaml
backend:
  service:
    name: kubarr-dashboard-oauth2-proxy
    port:
      number: 4180
```

**New:**
```yaml
backend:
  service:
    name: kubarr-oauth2-proxy
    port:
      number: 4180
```

## Verification

After migration, verify everything works:

1. **Check pods:**
   ```bash
   kubectl get pods -n kubarr-system
   ```

   You should see:
   - `kubarr-dashboard-xxx` (2/2 Running)
   - `kubarr-oauth2-proxy-xxx` (1/1 Running)

2. **Test OIDC discovery:**
   ```bash
   kubectl port-forward -n kubarr-system svc/kubarr-dashboard 8080:80
   curl http://localhost:8080/auth/.well-known/openid-configuration
   ```

3. **Test OAuth2 proxy:**
   ```bash
   kubectl port-forward -n kubarr-system svc/kubarr-oauth2-proxy 4180:4180
   curl http://localhost:4180/ping
   ```

4. **Test full authentication flow:**
   - Navigate to a protected route
   - Should redirect to login
   - Login should work and redirect back

## Rollback

If you need to rollback:

```bash
# Remove new OAuth2 proxy
helm uninstall kubarr-oauth2-proxy -n kubarr-system

# Rollback dashboard to previous version
helm rollback kubarr-dashboard -n kubarr-system
```

## Benefits of New Structure

1. **Modularity**: Deploy OAuth2 proxy independently of dashboard
2. **Flexibility**: Use different versions or configurations without affecting dashboard
3. **Clarity**: Clearer separation of concerns
4. **Reusability**: OAuth2 proxy chart can be used for other applications
5. **Easier Updates**: Update proxy without touching dashboard and vice versa

## Troubleshooting

### OAuth2 proxy can't connect to dashboard

Ensure the issuer URL in oauth2-proxy values matches the dashboard service:
```bash
kubectl get svc -n kubarr-system kubarr-dashboard
```

The issuer URL should be: `http://kubarr-dashboard.kubarr-system.svc.cluster.local:8000/auth`

### Secrets not found

Verify secrets exist in the correct namespace:
```bash
kubectl get secrets -n kubarr-system | grep oauth2
```

### Service name conflicts

If you see conflicts, ensure you've cleaned up old resources:
```bash
kubectl get all -n kubarr-system | grep oauth2-proxy
```

## Support

For issues during migration:
- Check logs: `kubectl logs -n kubarr-system -l app=oauth2-proxy`
- Open an issue: https://github.com/yourusername/kubarr/issues
