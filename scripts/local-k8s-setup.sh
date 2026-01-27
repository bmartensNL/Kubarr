#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOCAL_BIN="$PROJECT_ROOT/bin"

echo "=== Kubarr Local K8s Setup ==="

# Create local bin directory
mkdir -p "$LOCAL_BIN"
export PATH="$LOCAL_BIN:$PATH"

# Check for Docker
if ! command -v docker &> /dev/null; then
    echo "Error: Docker is required but not installed."
    exit 1
fi

# Install kind if not present
if ! command -v kind &> /dev/null; then
    echo "Installing kind to $LOCAL_BIN..."
    curl -Lo "$LOCAL_BIN/kind" https://kind.sigs.k8s.io/dl/v0.24.0/kind-linux-amd64
    chmod +x "$LOCAL_BIN/kind"
fi

# Install kubectl if not present
if ! command -v kubectl &> /dev/null; then
    echo "Installing kubectl to $LOCAL_BIN..."
    curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
    chmod +x kubectl
    mv kubectl "$LOCAL_BIN/kubectl"
fi

# Create kind cluster if not exists
if ! kind get clusters 2>/dev/null | grep -q "kubarr"; then
    echo "Creating kind cluster 'kubarr'..."
    kind create cluster --name kubarr --wait 60s
else
    echo "Kind cluster 'kubarr' already exists"
fi

# Set kubectl context
kubectl cluster-info --context kind-kubarr

# Build Docker image
echo "Building Docker image..."
cd "$PROJECT_ROOT/backend"
docker build -t kubarr:local .

# Load image into kind
echo "Loading image into kind cluster..."
kind load docker-image kubarr:local --name kubarr

# Apply Kubernetes manifests
echo "Deploying to Kubernetes..."
kubectl apply -f "$PROJECT_ROOT/k8s/namespace.yaml"
kubectl apply -f "$PROJECT_ROOT/k8s/rbac.yaml"
kubectl apply -f "$PROJECT_ROOT/k8s/service.yaml"
kubectl apply -f "$PROJECT_ROOT/k8s/deployment.yaml"

# Wait for deployment
echo "Waiting for deployment to be ready..."
kubectl rollout status deployment/kubarr -n kubarr --timeout=120s

echo ""
echo "=== Setup Complete ==="
echo ""
echo "To access Kubarr, run:"
echo "  kubectl port-forward -n kubarr svc/kubarr 8080:8080"
echo ""
echo "Then open: http://localhost:8080"
echo ""
echo "To view logs:"
echo "  kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr -f"
echo ""
echo "To delete the cluster:"
echo "  kind delete cluster --name kubarr"
