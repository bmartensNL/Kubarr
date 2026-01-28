# Claude Code Instructions for Kubarr

## Development Environment

### Kubernetes Setup
- Using Kind cluster named `kubarr`
- Backend runs in namespace `kubarr`

### Port Forwarding - ALWAYS DO THIS AFTER DEPLOY
**CRITICAL:** After EVERY backend deployment/restart, IMMEDIATELY run:

```bash
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &
```

Always verify it's working after redeployment:
```bash
curl -s http://localhost:8080/api/health
```

### Build and Deploy Backend
```bash
# Build Docker image
cd /home/bmartens/Projects/Kubarr
docker build -f docker/Dockerfile.backend -t kubarr-backend:latest --build-arg PROFILE=dev-release .

# Load into Kind cluster
kind load docker-image kubarr-backend:latest --name kubarr

# Restart deployment
kubectl rollout restart deployment/kubarr-backend -n kubarr
kubectl rollout status deployment/kubarr-backend -n kubarr --timeout=60s

# !!! CRITICAL - MUST restart port forward after EVERY deployment !!!
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 >/dev/null 2>&1 &
sleep 2
curl -s http://localhost:8080/api/health  # Verify it works
```

**NEVER FORGET:** The port-forward ALWAYS breaks after deployment. Run it immediately.
