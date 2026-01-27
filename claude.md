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
    +-- /api/*, /auth/* --> kubarr backend (port 8000)
    +-- /qbittorrent/*  --> qbittorrent service (port 8080)
    +-- /sonarr/*       --> sonarr service (port 8989)
    +-- /radarr/*       --> radarr service (port 7878)
    +-- /*              --> kubarr frontend (static files)
```

### Key Rules:
1. **ONLY port-forward oauth2-proxy to localhost** - never forward individual apps
2. **DO NOT add proxy router code to the dashboard backend** - nginx handles all routing
3. **kubarr only provides UI and API** for managing apps, NOT proxying to them
4. All apps are accessed through their path prefix (e.g., `/qbittorrent/`) via nginx

---

## IMPORTANT: Architecture Note

**Frontend and backend are separate pods/images!**

- **Frontend**: `Dockerfile.frontend` - nginx (Alpine) serving static files on port 80 with SPA routing
- **Backend**: `Dockerfile.backend` - Rust API server on port 8000

When you change frontend code, rebuild the **frontend** image.
When you change backend code, rebuild the **backend** image.

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
# Update the deployment image (backend)
kubectl set image deployment/kubarr-backend backend=kubarr-backend:<tag> -n kubarr

# Delete the pod to force recreation with new image
kubectl delete pod -l app.kubernetes.io/name=kubarr-backend -n kubarr

# Wait for rollout
kubectl rollout status deployment/kubarr-backend -n kubarr --timeout=120s
```

### Step 4: Verify the deployment

Always verify the new image is running:

```bash
# Check the version endpoint
kubectl exec deployment/kubarr-backend -n kubarr -c backend -- curl -s http://localhost:8000/api/system/version

# Check actual image being used
kubectl get pod -n kubarr -l app.kubernetes.io/name=kubarr-backend -o jsonpath='{.items[0].spec.containers[*].image}'
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

## Quick Deploy Scripts

### Deploy Frontend Only
```bash
TAG=$(date +%s) && \
docker build -f docker/Dockerfile.frontend -t kubarr-frontend:$TAG \
  --build-arg COMMIT_HASH=$(git rev-parse --short HEAD) \
  --build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ") . && \
docker save kubarr-frontend:$TAG | docker exec -i kubarr-test-control-plane ctr -n k8s.io images import - && \
kubectl set image deployment/kubarr-frontend frontend=kubarr-frontend:$TAG -n kubarr && \
kubectl delete pod -l app.kubernetes.io/name=kubarr-frontend -n kubarr && \
kubectl rollout status deployment/kubarr-frontend -n kubarr --timeout=60s
```

### Deploy Backend Only
```bash
TAG=$(date +%s) && \
docker build -f docker/Dockerfile.backend -t kubarr-backend:$TAG \
  --build-arg COMMIT_HASH=$(git rev-parse --short HEAD) \
  --build-arg BUILD_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ") . && \
docker save kubarr-backend:$TAG | docker exec -i kubarr-test-control-plane ctr -n k8s.io images import - && \
kubectl set image deployment/kubarr-backend backend=kubarr-backend:$TAG -n kubarr && \
kubectl delete pod -l app.kubernetes.io/name=kubarr-backend -n kubarr && \
kubectl rollout status deployment/kubarr-backend -n kubarr --timeout=120s
```

## Project Structure

- `code/frontend/` - React frontend (Vite + TypeScript)
- `code/backend/` - Rust backend (Axum)
- `charts/` - Helm charts for all apps
- `docker/` - Dockerfiles for frontend and backend

## Development

- Frontend dev server: `cd frontend && npm run dev`
- Backend: `cd kubarr-rs && cargo run`

## CRITICAL: Always Rebuild AND Deploy After Code Changes

**After making ANY changes to frontend or backend code, you MUST:**
1. Rebuild the appropriate Docker image
2. Load the image into Kind
3. Update the deployment and delete the pod
4. Verify the new build is running

**Changes that require frontend rebuild:** `code/frontend/` directory
**Changes that require backend rebuild:** `code/backend/` directory
