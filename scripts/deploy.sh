#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REMOTE_MODE=false
DOCKER_CONTEXT_FLAG=""
KUBECTL_CONTEXT_FLAG=""

usage() {
    echo "Usage: $0 [--remote]"
    echo ""
    echo "Build and deploy Kubarr to a Kind cluster."
    echo ""
    echo "Options:"
    echo "  --remote    Target remote Docker context (kubarr-remote) and Kind cluster"
    echo "  --help      Show this help message"
    echo ""
    echo "Without --remote, deploys to the local Kind cluster."
    echo "With --remote, uses the 'kubarr-remote' Docker context and 'kind-kubarr' kubectl context."
    echo ""
    echo "Prerequisites for remote mode:"
    echo "  Run scripts/remote-server-setup.sh first to configure the remote server."
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --remote)
            REMOTE_MODE=true
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Error: Unknown argument '$1'"
            usage
            exit 1
            ;;
    esac
done

if [ "$REMOTE_MODE" = true ]; then
    echo "=== Kubarr Deploy (Remote) ==="
    DOCKER_CONTEXT_FLAG="--context kubarr-remote"
    KUBECTL_CONTEXT_FLAG="--context kind-kubarr"

    # Verify remote Docker context exists
    if ! docker context ls --format '{{.Name}}' | grep -q '^kubarr-remote$'; then
        echo "Error: Docker context 'kubarr-remote' not found."
        echo "Run scripts/remote-server-setup.sh first to configure the remote server."
        exit 1
    fi

    # Verify remote Docker is reachable
    if ! docker $DOCKER_CONTEXT_FLAG info &> /dev/null; then
        echo "Error: Cannot connect to remote Docker daemon via 'kubarr-remote' context."
        echo "Check that the remote server is running and SSH access is working."
        exit 1
    fi

    # Warn if DOCKER_HOST is set (overrides context)
    if [ -n "${DOCKER_HOST:-}" ]; then
        echo "Warning: DOCKER_HOST is set ($DOCKER_HOST). This may override the Docker context."
        echo "Consider unsetting it: unset DOCKER_HOST"
    fi
else
    echo "=== Kubarr Deploy ==="
fi

cd "$PROJECT_ROOT"

# Build images
echo "Building backend..."
docker $DOCKER_CONTEXT_FLAG build -t kubarr-backend:latest -f docker/Dockerfile.backend --build-arg PROFILE=dev-release .

echo "Building frontend..."
docker $DOCKER_CONTEXT_FLAG build -t kubarr-frontend:latest -f docker/Dockerfile.frontend .

# Load into kind
# Note: kind respects the active Docker context, so when kubarr-remote is active,
# images are loaded into the remote Kind cluster containers
echo "Loading images into kind..."
if [ "$REMOTE_MODE" = true ]; then
    # Ensure kind uses the remote Docker context
    DOCKER_HOST_BACKUP="${DOCKER_HOST:-}"
    docker context use kubarr-remote > /dev/null 2>&1
fi

kind load docker-image kubarr-backend:latest --name kubarr
kind load docker-image kubarr-frontend:latest --name kubarr

if [ "$REMOTE_MODE" = true ]; then
    # Restore previous Docker context
    docker context use default > /dev/null 2>&1 || true
    if [ -n "$DOCKER_HOST_BACKUP" ]; then
        export DOCKER_HOST="$DOCKER_HOST_BACKUP"
    fi
fi

# Apply manifests
echo "Applying Kubernetes manifests..."
kubectl $KUBECTL_CONTEXT_FLAG apply -f k8s/

# Restart deployments to pick up new images
echo "Restarting deployments..."
kubectl $KUBECTL_CONTEXT_FLAG rollout restart deployment/kubarr-backend deployment/kubarr-frontend -n kubarr

# Wait for rollout
echo "Waiting for deployments..."
kubectl $KUBECTL_CONTEXT_FLAG rollout status deployment/kubarr-backend deployment/kubarr-frontend -n kubarr --timeout=120s

echo ""
echo "=== Deploy Complete ==="
echo ""
if [ "$REMOTE_MODE" = true ]; then
    echo "Deployed to remote Kind cluster via 'kubarr-remote' Docker context."
    echo ""
    echo "To access Kubarr, start port-forwarding:"
    echo "  kubectl --context kind-kubarr port-forward -n kubarr svc/kubarr-backend 8080:8000 &"
    echo ""
    echo "Then access Kubarr at: http://localhost:8080"
else
    echo "Access Kubarr at: http://localhost:8080"
fi
