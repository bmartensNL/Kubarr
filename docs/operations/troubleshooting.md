# Troubleshooting

Common issues and how to resolve them.

## 1. Pod Not Starting

### Symptoms

- `kubectl get pods -n kubarr` shows `Pending`, `ImagePullBackOff`, `ErrImagePull`, `OOMKilled`, or `CrashLoopBackOff`

### Diagnosis

```bash
# Check pod status and events
kubectl get pods -n kubarr
kubectl describe pod -n kubarr <pod-name>
kubectl get events -n kubarr --sort-by='.lastTimestamp'

# View container logs (current and previous restart)
kubectl logs -n kubarr deployment/kubarr-backend
kubectl logs -n kubarr deployment/kubarr-backend --previous
```

### Fixes by error type

| Error | Cause | Fix |
|-------|-------|-----|
| `ImagePullBackOff` / `ErrImagePull` | Image not found or registry auth missing | Check image tag in `helm get values kubarr -n kubarr`. Add `imagePullSecrets` if using a private registry. |
| `OOMKilled` | Container exceeded memory limit | Increase `backend.resources.limits.memory` in Helm values and upgrade. |
| `CrashLoopBackOff` | Application crash on startup | Check logs with `--previous` flag. Most commonly: database connection failure or missing `KUBARR_JWT_SECRET`. |
| `Pending` (no node) | Insufficient cluster resources | Check node capacity: `kubectl describe nodes`. Scale up the cluster or lower resource requests. |

---

## 2. Can't Access the UI

### Symptoms

- Browser shows connection refused or times out on `http://localhost:8080`
- Port-forward command exits unexpectedly

### Diagnosis

```bash
# Check services exist
kubectl get svc -n kubarr

# Check frontend pod is running
kubectl get pods -n kubarr -l app.kubernetes.io/component=frontend

# Check if port-forward is still running
ps aux | grep port-forward
```

### Fixes

**Port-forward not running:** The port-forward process terminates when pods restart. Restart it:

```bash
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &
```

**Service not exposed:** If using NodePort or LoadBalancer, verify the service type:

```bash
kubectl get svc kubarr-frontend -n kubarr -o wide
```

If the `EXTERNAL-IP` shows `<pending>` on a LoadBalancer service, your cluster may not have a load balancer controller. Use NodePort or port-forward instead.

**NodePort blocked:** Ensure the node's firewall or security group allows traffic on the NodePort (default: 30080). On cloud providers, add an inbound rule for the port.

---

## 3. Login Failing

### Symptoms

- "Invalid credentials" error with correct username/password
- "Session expired" banner after a short time
- Login page keeps reloading

### Diagnosis

```bash
# Check backend logs for auth errors
kubectl logs -n kubarr deployment/kubarr-backend | grep -i "auth\|login\|session\|error"

# Verify database is reachable from backend
kubectl exec -n kubarr deployment/kubarr-backend -- \
  env | grep DATABASE_URL
```

### Fixes

**Wrong credentials:** Use the default admin credentials set during setup. If forgotten, reset via the database:

```bash
# Connect to the PostgreSQL pod
kubectl exec -it -n kubarr deployment/kubarr-postgres -- \
  psql -U kubarr -d kubarr

-- Reset password (replace with bcrypt hash of new password)
UPDATE users SET password_hash = '<new-bcrypt-hash>' WHERE username = 'admin';
```

**Session expired:** Access tokens expire after 1 hour by default. Increase `auth.jwt.accessTokenExpire` in Helm values if needed.

**Database connection issue:** Check the backend can reach the database:

```bash
kubectl logs -n kubarr deployment/kubarr-backend | grep -i "database\|connection\|migration"
```

Look for `KUBARR_DATABASE_URL` being set correctly in the pod environment:

```bash
kubectl exec -n kubarr deployment/kubarr-backend -- env | grep KUBARR_DATABASE
```

---

## 4. App Installation Failing

### Symptoms

- Application shows "Failed" status in the dashboard
- Helm install/upgrade error in backend logs
- Application namespace created but pods not starting

### Diagnosis

```bash
# Check backend logs for Helm errors
kubectl logs -n kubarr deployment/kubarr-backend | grep -i "helm\|install\|error"

# Check the application's own namespace
kubectl get all -n <app-namespace>
kubectl get events -n <app-namespace> --sort-by='.lastTimestamp'
kubectl describe pod -n <app-namespace> <pod-name>
```

### Fixes

**Helm error — chart not found:** Verify the OCI registry is reachable from the cluster:

```bash
helm pull oci://ghcr.io/bmartensnl/kubarr/charts/kubarr --version 0.1.0
```

**Namespace conflict:** If a namespace with the same name already exists from a previous failed install:

```bash
# List Helm releases in the namespace
helm list -n <app-namespace>

# If no release exists, delete and retry
kubectl delete namespace <app-namespace>
```

**Image pull failure for the app:** Check if the app image is accessible:

```bash
kubectl describe pod -n <app-namespace> <pod-name> | grep -A5 "Events:"
```

Add `imagePullSecrets` to the application's Helm values if it uses a private registry.

---

## 5. High Resource Usage

### Symptoms

- Node is under memory or CPU pressure
- Pods being evicted or throttled
- Dashboard is slow or unresponsive

### Diagnosis

```bash
# Top resource consumers by pod
kubectl top pods -n kubarr
kubectl top pods --all-namespaces | sort -k4 -hr | head -20

# Check node resource pressure
kubectl top nodes
kubectl describe node <node-name> | grep -A10 "Conditions:"

# Check resource limits and requests for kubarr
kubectl get pods -n kubarr -o json | \
  jq '.items[].spec.containers[].resources'
```

### Fixes

**Backend using too much memory:** Increase limits or investigate via profiling:

```bash
# Increase backend memory limit via Helm upgrade
helm upgrade kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
  -n kubarr \
  --set backend.resources.limits.memory=1Gi \
  --wait
```

**Media app using excessive resources:** Set resource limits for the application in its Helm values. Access the app configuration from the Kubarr dashboard → Applications → Settings.

**No resource limits set:** Pods without limits can consume all node resources. Always set limits:

```yaml
# values.yaml
backend:
  resources:
    limits:
      cpu: 500m
      memory: 512Mi
    requests:
      cpu: 100m
      memory: 256Mi
```

---

## 6. Log Collection

### Kubarr Backend Logs

```bash
# Stream live logs
kubectl logs -f -n kubarr deployment/kubarr-backend

# Last 100 lines
kubectl logs -n kubarr deployment/kubarr-backend --tail=100

# Logs from previous crash
kubectl logs -n kubarr deployment/kubarr-backend --previous

# Save to file
kubectl logs -n kubarr deployment/kubarr-backend > kubarr-backend.log
```

### Kubarr Frontend Logs

```bash
kubectl logs -f -n kubarr deployment/kubarr-frontend
```

### Installed Application Logs

```bash
# List pods in the application namespace
kubectl get pods -n <app-namespace>

# Stream logs from an app pod
kubectl logs -f -n <app-namespace> <pod-name>

# All containers in a pod (if multi-container)
kubectl logs -f -n <app-namespace> <pod-name> --all-containers
```

### Collecting All Logs for Bug Reports

```bash
#!/bin/bash
# Collect diagnostics into a single archive

mkdir -p kubarr-diag

kubectl get all -n kubarr > kubarr-diag/resources.txt
kubectl get events -n kubarr --sort-by='.lastTimestamp' > kubarr-diag/events.txt
kubectl logs -n kubarr deployment/kubarr-backend > kubarr-diag/backend.log
kubectl logs -n kubarr deployment/kubarr-frontend > kubarr-diag/frontend.log
kubectl describe pods -n kubarr > kubarr-diag/pod-details.txt

tar -czf kubarr-diagnostics.tar.gz kubarr-diag/
echo "Diagnostics saved to kubarr-diagnostics.tar.gz"
```

---

## Getting More Help

- [GitHub Issues](https://github.com/bmartensNL/Kubarr/issues) — Bug reports and feature requests
- [GitHub Discussions](https://github.com/bmartensNL/Kubarr/discussions) — Community support
- [Upgrade Guide](upgrade.md) — Upgrade-specific troubleshooting
