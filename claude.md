# Claude Code Instructions for Kubarr

## CRITICAL: Traffic Flow Architecture

**ALL traffic flows through: User -> oauth2-proxy -> nginx -> app**

```
User (localhost:8080)
    |
    v
oauth2-proxy (handles authentication)
    |
    v
nginx (routes requests based on path)
    |
    +-- /api/*, /auth/* --> kubarr-dashboard backend (port 8000)
    +-- /qbittorrent/*  --> qbittorrent service (port 8080)
    +-- /sonarr/*       --> sonarr service (port 8989)
    +-- /radarr/*       --> radarr service (port 7878)
    +-- /*              --> kubarr-dashboard frontend (static files)
```

### Key Rules:
1. **ONLY port-forward oauth2-proxy to localhost** - never forward individual apps
2. **DO NOT add proxy router code to the dashboard backend** - nginx handles all routing
3. **kubarr-dashboard only provides UI and API** for managing apps, NOT proxying to them
4. All apps are accessed through their path prefix (e.g., `/qbittorrent/`) via nginx

---

## IMPORTANT: Architecture Note

**The backend container serves the frontend static files!**

The `Dockerfile.backend` has a multi-stage build that:
1. Builds the frontend in a Node.js stage
2. Copies the built frontend to `/app/static` in the Python stage
3. Serves static files from the backend via FastAPI

This means: **When you change frontend code, you MUST rebuild the BACKEND image, not the frontend image.**

The separate `Dockerfile.frontend` (nginx-based) exists but is NOT used for serving in the current deployment.

## Deploying to Kind Cluster

When deploying Docker images to the Kind cluster, follow these steps **exactly** to ensure the new image is actually used:

### Step 1: Build the Docker image with a unique tag

Always use a timestamp-based tag to ensure uniqueness:

```bash
# Frontend
docker build -f docker/Dockerfile.frontend -t kubarr-frontend:$(date +%s) \
  --build-arg COMMIT_HASH=$(git rev-parse --short HEAD) \
  --build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ") .

# Backend
docker build -f docker/Dockerfile.backend -t kubarr-backend:$(date +%s) \
  --build-arg COMMIT_HASH=$(git rev-parse --short HEAD) \
  --build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ") .
```

### Step 2: Load the image into Kind

Use `docker save` piped to `ctr images import` for reliable loading:

```bash
docker save kubarr-frontend:<tag> | docker exec -i kubarr-test-control-plane ctr -n k8s.io images import -
```

**DO NOT use `kind load docker-image`** - it has caching issues.

### Step 3: Update the deployment AND delete the pod

Simply updating the image is NOT enough. You must also delete the existing pod:

```bash
# Update the deployment image
kubectl set image deployment/kubarr-dashboard frontend=kubarr-frontend:<tag> -n kubarr-system

# Delete the pod to force recreation with new image
kubectl delete pod -l app=kubarr-dashboard -n kubarr-system

# Wait for rollout
kubectl rollout status deployment/kubarr-dashboard -n kubarr-system --timeout=60s
```

### Step 4: Verify the deployment

Always verify the new image is running:

```bash
# Check the version endpoint
kubectl exec deployment/kubarr-dashboard -n kubarr-system -c backend -- curl -s http://localhost:8000/api/system/version

# Check actual image being used
kubectl get pod -n kubarr-system -l app=kubarr-dashboard -o jsonpath='{.items[0].spec.containers[*].image}'
```

## Common Issues

### Image not updating despite rebuild

**Cause**: Kind caches images by digest, not just tag. Even with a new tag, the old image may be used.

**Solution**:
1. Use timestamp-based tags (e.g., `kubarr-frontend:1706025600`)
2. Always delete the pod after updating the deployment
3. Use `ctr images import` instead of `kind load`

### Frontend showing "unknown" for commit hash

**Cause**: The VITE_COMMIT_HASH build arg wasn't passed or the old image is cached.

**Solution**: Ensure build args are passed:
```bash
--build-arg COMMIT_HASH=$(git rev-parse --short HEAD)
--build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
```

## Quick Deploy Script

For convenience, here's a one-liner to rebuild and deploy (frontend changes require backend rebuild):

```bash
TAG=$(date +%s) && \
docker build -f docker/Dockerfile.backend -t kubarr-backend:$TAG \
  --build-arg COMMIT_HASH=$(git rev-parse --short HEAD) \
  --build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ") . && \
docker save kubarr-backend:$TAG | docker exec -i kubarr-test-control-plane ctr -n k8s.io images import - && \
kubectl set image deployment/kubarr-dashboard backend=kubarr-backend:$TAG -n kubarr-system && \
kubectl delete pod -l app=kubarr-dashboard -n kubarr-system && \
kubectl rollout status deployment/kubarr-dashboard -n kubarr-system --timeout=90s
```

## Project Structure

- `frontend/` - React frontend (Vite + TypeScript)
- `kubarr/` - Python backend (FastAPI)
- `charts/` - Helm charts for all apps
- `docker/` - Dockerfiles for frontend and backend

## Development

- Frontend dev server: `cd frontend && npm run dev`
- Backend dev server: `cd kubarr && uvicorn api.main:app --reload`
