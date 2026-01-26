$ErrorActionPreference = "Stop"
Set-Location "C:\Users\admin\Projects\Kubarr"

$TAG = [int][double]::Parse((Get-Date -UFormat '%s'))
$COMMIT = git rev-parse --short HEAD
$BUILD_TIME = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"

Write-Host "Building with tag: $TAG, commit: $COMMIT" -ForegroundColor Cyan

# Build backend
Write-Host "`n=== Building Backend ===" -ForegroundColor Yellow
docker build -f "C:\Users\admin\Projects\Kubarr\docker\Dockerfile.backend" -t "kubarr-backend:$TAG" `
    --build-arg "COMMIT_HASH=$COMMIT" `
    --build-arg "BUILD_TIME=$BUILD_TIME" "C:\Users\admin\Projects\Kubarr"
if ($LASTEXITCODE -ne 0) { throw "Backend build failed" }

# Build frontend
Write-Host "`n=== Building Frontend ===" -ForegroundColor Yellow
docker build -f "C:\Users\admin\Projects\Kubarr\docker\Dockerfile.frontend" -t "kubarr-frontend:$TAG" `
    --build-arg "COMMIT_HASH=$COMMIT" `
    --build-arg "BUILD_TIME=$BUILD_TIME" "C:\Users\admin\Projects\Kubarr"
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }

# Load images into Kind
Write-Host "`n=== Loading images into Kind ===" -ForegroundColor Yellow
kind load docker-image "kubarr-backend:$TAG" --name kubarr-test
kind load docker-image "kubarr-frontend:$TAG" --name kubarr-test

# Update deployments
Write-Host "`n=== Updating deployments ===" -ForegroundColor Yellow
kubectl set image deployment/kubarr backend="kubarr-backend:$TAG" -n kubarr
kubectl set image deployment/kubarr-frontend frontend="kubarr-frontend:$TAG" -n kubarr

# Delete pods to force recreation
Write-Host "`n=== Restarting pods ===" -ForegroundColor Yellow
kubectl delete pod -l app.kubernetes.io/name=kubarr -n kubarr
kubectl delete pod -l app.kubernetes.io/name=kubarr-frontend -n kubarr

# Wait for rollout
Write-Host "`n=== Waiting for rollout ===" -ForegroundColor Yellow
kubectl rollout status deployment/kubarr -n kubarr --timeout=120s
kubectl rollout status deployment/kubarr-frontend -n kubarr --timeout=60s

Write-Host "`n=== Deployment complete! ===" -ForegroundColor Green
Write-Host "Backend: kubarr-backend:$TAG"
Write-Host "Frontend: kubarr-frontend:$TAG"
