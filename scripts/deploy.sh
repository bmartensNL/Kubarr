#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Kubarr Deploy ==="

cd "$PROJECT_ROOT"

# Build images
echo "Building backend..."
docker build -t kubarr-backend:latest -f code/backend/Dockerfile .

echo "Building frontend..."
docker build -t kubarr-frontend:latest -f docker/Dockerfile.frontend .

# Load into kind
echo "Loading images into kind..."
kind load docker-image kubarr-backend:latest --name kubarr
kind load docker-image kubarr-frontend:latest --name kubarr

# Apply manifests
echo "Applying Kubernetes manifests..."
kubectl apply -f k8s/

# Restart deployments to pick up new images
echo "Restarting deployments..."
kubectl rollout restart deployment/kubarr-backend deployment/kubarr-frontend -n kubarr

# Wait for rollout
echo "Waiting for deployments..."
kubectl rollout status deployment/kubarr-backend deployment/kubarr-frontend -n kubarr --timeout=120s

echo ""
echo "=== Deploy Complete ==="
echo ""
echo "Access Kubarr at: http://localhost:8080"
