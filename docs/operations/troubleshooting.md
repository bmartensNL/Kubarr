# Troubleshooting

Common issues, their diagnosis commands, and fixes.

---

## 1. Pod not starting

### Symptoms

- `kubectl get pods -n kubarr` shows `ImagePullBackOff`, `OOMKilled`, `CrashLoopBackOff`, or `Pending`

### Diagnosis

```bash
# List pod status and restart counts
kubectl get pods -n kubarr

# Show events for a pod (image pull errors, OOM, etc.)
kubectl describe pod <pod-name> -n kubarr

# Show recent logs (useful for CrashLoopBackOff)
kubectl logs <pod-name> -n kubarr --previous
```

### Fixes

| Reason | Fix |
|--------|-----|
| `ImagePullBackOff` | Verify the image tag exists. Check `kubectl describe pod` for the exact image name. Ensure the node can reach the registry (GHCR). |
| `OOMKilled` | The pod exceeded its memory limit. Increase `resources.limits.memory` in your Helm values: `helm upgrade kubarr ... --set backend.resources.limits.memory=1Gi` |
| `CrashLoopBackOff` | Check `kubectl logs <pod> --previous` for a panic or config error. Common cause: bad `DATABASE_URL` or missing secret. |
| `Pending` | No node has enough CPU/memory, or no matching node selector/tolerations. Run `kubectl describe pod <pod>` and check the `Events` section for `Insufficient cpu` / `Insufficient memory`. |

---

## 2. Can't access the UI

### Symptoms

- Browser shows connection refused or times out at the expected URL
- `kubectl port-forward` command has exited

### Diagnosis

```bash
# Check frontend pod is running
kubectl get pods -n kubarr -l app=kubarr-frontend

# Check service exists
kubectl get svc -n kubarr

# Try port-forwarding manually
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
curl -I http://localhost:8080
```

### Fixes

| Scenario | Fix |
|----------|-----|
| Port-forward not running | Re-run: `kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &` |
| NodePort blocked by firewall | Open the node port in your firewall: `ufw allow <nodePort>/tcp` or equivalent. Find the port with `kubectl get svc kubarr-frontend -n kubarr`. |
| LoadBalancer IP pending | In bare-metal clusters, `EXTERNAL-IP` stays `<pending>` without a load-balancer controller. Use port-forward, NodePort, or install MetalLB. |
| Ingress misconfigured | Check `kubectl describe ingress -n kubarr` for TLS or backend errors. Verify the ingress controller is running. |

---

## 3. Login failing

### Symptoms

- "Invalid username or password" after entering correct credentials
- "Session expired" immediately after login
- Blank page after login redirect

### Diagnosis

```bash
# Check backend logs for auth errors
kubectl logs -n kubarr deploy/kubarr-backend | grep -i "auth\|login\|session\|error"

# Verify database connectivity
kubectl exec -n kubarr deploy/kubarr-backend -- \
  curl -s http://localhost:8000/api/health | jq .database
```

### Fixes

| Cause | Fix |
|-------|-----|
| Wrong credentials | Reset via the admin user: `kubectl exec -n kubarr deploy/kubarr-backend -- /usr/local/bin/kubarr reset-password --user admin` (if the reset command is available), or update the `hashed_password` column directly in the DB. |
| Account not approved | New accounts require admin approval. Log in as admin, go to **Settings → Users**, and approve the account. |
| Session expired | Log in again. Adjust session duration in **Settings → System** if sessions expire too quickly. |
| DB connection refused | Verify `DATABASE_URL` secret is correct: `kubectl get secret kubarr-db-secret -n kubarr -o jsonpath='{.data.DATABASE_URL}' | base64 -d` |
| Clock skew causing JWT rejection | Ensure system time on Kubernetes nodes is synced (`timedatectl status`). |

---

## 4. App installation failing

### Symptoms

- Install button spins indefinitely
- `/api/apps/install` returns an error
- App pod stays in `Pending` or `ImagePullBackOff` after install

### Diagnosis

```bash
# Check backend logs during install
kubectl logs -n kubarr deploy/kubarr-backend -f | grep -i "helm\|install\|error"

# Check events in the app's namespace
kubectl get events -n <app-name> --sort-by='.lastTimestamp'

# Check if namespace was created
kubectl get ns <app-name>

# Check pods in app namespace
kubectl get pods -n <app-name>
```

### Fixes

| Cause | Fix |
|-------|-----|
| Helm binary not found | Ensure `helm` is installed in the backend container or on the node PATH. Check the Helm chart config in backend settings. |
| Namespace conflict | A namespace with the app name already exists from a failed previous install. Run `kubectl delete ns <app-name>` and try again. |
| Image pull failure | The app's container image may be unavailable or rate-limited. Check `kubectl describe pod -n <app-name>` for the exact error. |
| Charts not synced | Trigger a manual chart sync: `POST /api/apps/sync` (requires `AppsInstall` permission). |
| Insufficient cluster resources | Check available capacity: `kubectl describe nodes | grep -A5 "Allocated resources"` |

---

## 5. High resource usage

### Symptoms

- Dashboard shows a pod consuming excessive CPU or memory
- Kubernetes node is under pressure; other pods are being evicted

### Diagnosis

```bash
# Top pods across all namespaces
kubectl top pods -A

# Top pods in a specific app namespace
kubectl top pods -n <app-name>

# Top nodes
kubectl top nodes

# Check resource requests and limits
kubectl describe pod <pod-name> -n <app-name> | grep -A6 "Limits\|Requests"
```

### Fixes

| Action | Command |
|--------|---------|
| Set/increase memory limit for an app | Via the Kubarr UI: **Apps → \<app\> → Settings → Resources** |
| Reduce CPU limit for a specific app | `helm upgrade <app> <chart> -n <app> --set resources.limits.cpu=500m` |
| Restart a misbehaving pod | `kubectl rollout restart deployment/<app> -n <app>` or use the **Restart** button in the Kubarr UI |
| Check if an app has a memory leak | `kubectl top pod -n <app-name> --containers` — watch memory growing over time |

---

## 6. Log collection

### Backend logs

```bash
# Live tail
kubectl logs -n kubarr deploy/kubarr-backend -f

# Last 500 lines
kubectl logs -n kubarr deploy/kubarr-backend --tail=500

# Filter for errors only
kubectl logs -n kubarr deploy/kubarr-backend | grep -i "error\|panic\|warn"
```

### Frontend logs

The BusyBox frontend serves static files; most errors will appear in the browser developer console (F12). To check if the frontend container itself is running:

```bash
kubectl logs -n kubarr deploy/kubarr-frontend
```

### Installed app logs

```bash
# List pods in the app namespace
kubectl get pods -n <app-name>

# Stream logs from a specific pod
kubectl logs -n <app-name> <pod-name> -f

# If the pod has multiple containers
kubectl logs -n <app-name> <pod-name> -c <container-name> -f
```

### Collecting a diagnostics bundle

To collect logs from all Kubarr-related namespaces in one go:

```bash
#!/usr/bin/env bash
set -euo pipefail
OUTDIR="kubarr-diagnostics-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUTDIR"

# Kubarr system pods
for ns in kubarr; do
  kubectl get pods -n "$ns" -o wide > "$OUTDIR/${ns}-pods.txt"
  kubectl get events -n "$ns" --sort-by='.lastTimestamp' > "$OUTDIR/${ns}-events.txt"
  for pod in $(kubectl get pods -n "$ns" -o name); do
    kubectl logs -n "$ns" "$pod" --all-containers=true \
      > "$OUTDIR/${ns}-$(basename $pod).log" 2>&1 || true
  done
done

# Installed app namespaces (skip system namespaces)
for ns in $(kubectl get ns -o name | grep -v "kube-\|default\|kubarr" | sed 's|namespace/||'); do
  kubectl get events -n "$ns" --sort-by='.lastTimestamp' > "$OUTDIR/${ns}-events.txt" 2>/dev/null || true
  kubectl get pods -n "$ns" -o wide > "$OUTDIR/${ns}-pods.txt" 2>/dev/null || true
done

tar czf "${OUTDIR}.tar.gz" "$OUTDIR"
echo "Bundle created: ${OUTDIR}.tar.gz"
```
