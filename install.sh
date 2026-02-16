#!/bin/bash
set -e

# Kubarr installation script
# Usage: curl -sfL https://raw.githubusercontent.com/bmartensNL/Kubarr/main/install.sh | sh -

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
KUBARR_NAMESPACE="kubarr"
KUBARR_VERSION="${KUBARR_VERSION:-latest}"
MIN_RAM_MB=2048

# Print functions
print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_header() {
    echo ""
    echo "╔════════════════════════════════════════════╗"
    echo "║          Kubarr Installation Script        ║"
    echo "║   Kubernetes Dashboard for Media Servers   ║"
    echo "╚════════════════════════════════════════════╝"
    echo ""
}

# Check if running as root
check_root() {
    if [ "$EUID" -eq 0 ]; then
        print_warning "Running as root. k3s installation will proceed with root privileges."
    else
        print_info "Running as non-root user. You may be prompted for sudo password."
    fi
}

# Detect OS and architecture
detect_system() {
    print_info "Detecting system information..."

    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$ARCH" in
        x86_64)
            ARCH="amd64"
            ;;
        aarch64|arm64)
            ARCH="arm64"
            ;;
        armv7l)
            ARCH="arm"
            ;;
        *)
            print_error "Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac

    print_success "Detected: $OS/$ARCH"

    # Check RAM
    if [ "$OS" = "linux" ]; then
        TOTAL_RAM=$(free -m | awk '/^Mem:/{print $2}')
        if [ "$TOTAL_RAM" -lt "$MIN_RAM_MB" ]; then
            print_warning "System has ${TOTAL_RAM}MB RAM. Recommended: ${MIN_RAM_MB}MB or more."
            print_warning "Kubarr may run slowly with limited resources."
            read -p "Continue anyway? (y/N) " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        fi
    fi
}

# Check prerequisites
check_prerequisites() {
    print_info "Checking prerequisites..."

    # Check if curl is available
    if ! command -v curl &> /dev/null; then
        print_error "curl is required but not installed."
        exit 1
    fi

    print_success "Prerequisites satisfied"
}

# Install k3s
install_k3s() {
    if command -v k3s &> /dev/null; then
        print_success "k3s is already installed"
        K3S_VERSION=$(k3s --version | head -n1 | cut -d' ' -f3)
        print_info "Version: $K3S_VERSION"
        return
    fi

    print_info "Installing k3s (lightweight Kubernetes)..."
    print_info "This may take a few minutes..."

    # Install k3s with minimal components
    curl -sfL https://get.k3s.io | sh -s - \
        --write-kubeconfig-mode 644 \
        --disable traefik \
        --disable servicelb

    print_success "k3s installed successfully"
}

# Wait for k3s to be ready
wait_for_k3s() {
    print_info "Waiting for Kubernetes to be ready..."

    # Set KUBECONFIG
    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

    # Wait for node to be ready (max 60 seconds)
    for i in {1..60}; do
        if kubectl get nodes 2>/dev/null | grep -q "Ready"; then
            print_success "Kubernetes is ready"
            return
        fi
        sleep 1
    done

    print_error "Kubernetes did not become ready in time"
    exit 1
}

# Install Helm if not present
install_helm() {
    if command -v helm &> /dev/null; then
        print_success "Helm is already installed"
        return
    fi

    print_info "Installing Helm..."
    curl -sfL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash
    print_success "Helm installed successfully"
}

# Deploy Kubarr
deploy_kubarr() {
    print_info "Deploying Kubarr to Kubernetes..."

    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

    # Create namespace
    kubectl create namespace "$KUBARR_NAMESPACE" 2>/dev/null || true

    # Add Helm repo or install from OCI
    print_info "Installing Kubarr Helm chart..."

    if [ "$KUBARR_VERSION" = "latest" ]; then
        helm install kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
            -n "$KUBARR_NAMESPACE" \
            --wait \
            --timeout 5m
    else
        helm install kubarr oci://ghcr.io/bmartensnl/kubarr/charts/kubarr \
            --version "$KUBARR_VERSION" \
            -n "$KUBARR_NAMESPACE" \
            --wait \
            --timeout 5m
    fi

    print_success "Kubarr deployed successfully"
}

# Setup access
setup_access() {
    print_info "Setting up access to Kubarr..."

    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

    # Check if service exists
    if ! kubectl get svc kubarr-frontend -n "$KUBARR_NAMESPACE" &>/dev/null; then
        print_warning "Frontend service not found. Installation may be incomplete."
        return
    fi

    # Get service port
    SERVICE_PORT=$(kubectl get svc kubarr-frontend -n "$KUBARR_NAMESPACE" -o jsonpath='{.spec.ports[0].port}')

    print_success "Kubarr is accessible via port-forward"
    print_info "To access the dashboard, run:"
    echo ""
    echo "  kubectl port-forward -n $KUBARR_NAMESPACE svc/kubarr-frontend 8080:$SERVICE_PORT"
    echo ""
    print_info "Then open: http://localhost:8080"
}

# Get credentials
get_credentials() {
    print_info "Retrieving access credentials..."

    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

    # Wait for backend pod to be ready
    kubectl wait --for=condition=ready pod -l app=kubarr-backend -n "$KUBARR_NAMESPACE" --timeout=60s &>/dev/null || true

    # Try to get default credentials from deployment
    # Note: This depends on how Kubarr handles initial setup
    print_info "Default credentials will be shown on first login"
    print_warning "Make sure to change the default password after first login!"
}

# Print completion message
print_completion() {
    echo ""
    echo "╔════════════════════════════════════════════╗"
    echo "║     Kubarr Installation Complete! 🎉      ║"
    echo "╚════════════════════════════════════════════╝"
    echo ""
    print_success "Installation successful!"
    echo ""
    echo "Next steps:"
    echo ""
    echo "1. Start port forwarding:"
    echo "   ${GREEN}kubectl port-forward -n $KUBARR_NAMESPACE svc/kubarr-frontend 8080:80${NC}"
    echo ""
    echo "2. Open your browser:"
    echo "   ${GREEN}http://localhost:8080${NC}"
    echo ""
    echo "3. Log in with default credentials (shown on first access)"
    echo ""
    echo "Useful commands:"
    echo "  • Check status:    ${BLUE}kubectl get pods -n $KUBARR_NAMESPACE${NC}"
    echo "  • View logs:       ${BLUE}kubectl logs -n $KUBARR_NAMESPACE -l app=kubarr-backend${NC}"
    echo "  • Uninstall:       ${BLUE}helm uninstall kubarr -n $KUBARR_NAMESPACE${NC}"
    echo ""
    echo "Documentation: https://github.com/bmartensNL/Kubarr/tree/main/docs"
    echo ""
}

# Main installation flow
main() {
    print_header
    check_root
    detect_system
    check_prerequisites
    install_k3s
    wait_for_k3s
    install_helm
    deploy_kubarr
    setup_access
    get_credentials
    print_completion
}

# Run main function
main
