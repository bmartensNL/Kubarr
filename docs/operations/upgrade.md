# Upgrading Kubarr

Kubarr follows semantic versioning. This guide covers all upgrade paths.

## Using the Install Script (Recommended)

The install script is idempotent — re-running it upgrades Kubarr to the latest release:

```bash
curl -sfL https://raw.githubusercontent.com/bmartensNL/Kubarr/main/install.sh | sh -
```

The script will:

1. Detect the latest published version
2. Run `helm upgrade` (or `helm install` on a fresh cluster)
3. Wait for pods to become ready

## Manual Helm Upgrade

If you installed via Helm directly, upgrade with:

```bash
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  --wait
```

To upgrade to a specific version:

```bash
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  --version 0.2.0 \
  --wait
```

To preserve your existing custom values during upgrade:

```bash
# Export current values first
helm get values kubarr -n kubarr > current-values.yaml

# Upgrade with preserved values
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  -f current-values.yaml \
  --wait
```

## Database Migrations

Database migrations run **automatically on backend startup**. No manual steps are required.

Migrations are applied in order and are idempotent — running the same migration twice is safe. If a migration fails, the backend will exit with an error and log the failure. Check backend logs if the pod enters `CrashLoopBackOff`:

```bash
kubectl logs -n kubarr deployment/kubarr-backend --previous
```

## Rollback

To roll back to the previous Helm release revision:

```bash
helm rollback kubarr -n kubarr
```

To roll back to a specific revision:

```bash
# List revision history
helm history kubarr -n kubarr

# Roll back to revision 2
helm rollback kubarr 2 -n kubarr
```

!!! warning "Database rollbacks"
    Helm rollback restores the previous application version, but **does not reverse database migrations**. If the new version added schema changes, the old application may fail to start against the new schema. Test upgrades in a staging environment before rolling back production.

## Verifying the Upgrade

After upgrading, confirm the new version is running:

```bash
# Check pod status
kubectl get pods -n kubarr

# Check the running image tag
kubectl get deployment kubarr-backend -n kubarr \
  -o jsonpath='{.spec.template.spec.containers[0].image}'

# Verify health endpoint
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &
curl -s http://localhost:8080/api/health | jq .
```

## See Also

- [Troubleshooting](troubleshooting.md) — Common issues after upgrades
- [Configuration Reference](../configuration.md) — Helm values reference
