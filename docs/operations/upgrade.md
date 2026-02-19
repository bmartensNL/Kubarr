# Upgrading Kubarr

## Using the install script (recommended)

The install script is idempotent: re-running it upgrades Kubarr to the latest released version.

```bash
curl -sfL https://raw.githubusercontent.com/bmartensNL/Kubarr/main/install.sh | sh -
```

The script will:

1. Detect your existing k3s / Kubernetes setup
2. Pull the latest Helm chart from the OCI registry
3. Run `helm upgrade --install` with your existing values preserved
4. Wait for the rollout to complete

---

## Manual Helm upgrade

If you manage Kubarr with Helm directly, run:

```bash
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  --reuse-values \
  --wait
```

`--reuse-values` re-applies your current Helm values so no configuration is lost.

To upgrade **and** change a value at the same time:

```bash
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  --reuse-values \
  --set backend.image.tag=0.2.0 \
  --wait
```

---

## Database migrations

Migrations run **automatically on startup**. No manual steps are needed.

When the backend pod starts, SeaORM compares the current schema version to the pending migrations and applies any new ones in order. The process is:

1. Backend pod starts
2. Backend connects to PostgreSQL
3. SeaORM migration runner checks the `seaql_migrations` table
4. Any unapplied migrations are executed in sequence
5. Backend begins serving requests

!!! warning
    Always back up your database before upgrading to a new major version.

    ```bash
    # Example: pg_dump from the running pod
    kubectl exec -n kubarr deploy/kubarr-backend -- \
      pg_dump "$DATABASE_URL" > kubarr-backup-$(date +%Y%m%d).sql
    ```

---

## Rollback

To roll back to the previous Helm release revision:

```bash
# List available revisions
helm history kubarr -n kubarr

# Roll back to the previous revision
helm rollback kubarr -n kubarr

# Roll back to a specific revision number
helm rollback kubarr 3 -n kubarr
```

!!! note
    Helm rollback reverts the Kubernetes workloads to the previous image and config, but **does not reverse database migrations**. If a migration added a column, that column remains after rollback — the older code will simply ignore it. Dropping columns or tables during a rollback is a manual operation.

---

## Post-upgrade verification

After any upgrade, verify the deployment is healthy:

```bash
# Check pod status
kubectl get pods -n kubarr

# Check rollout completed
kubectl rollout status deployment/kubarr-backend -n kubarr

# Check backend health endpoint
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &
curl -s http://localhost:8080/api/health | jq .

# Check current deployed version
helm list -n kubarr
```
