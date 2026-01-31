# Kubarr Installation Guide

This guide provides detailed instructions for installing Kubarr on various Kubernetes platforms, from local development clusters to production-grade managed Kubernetes services.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Installation Methods](#installation-methods)
  - [Kind (Local Development)](#kind-local-development)
  - [Kind (Remote Build Server)](#kind-remote-build-server)
  - [k3s (Lightweight Kubernetes)](#k3s-lightweight-kubernetes)
  - [Managed Kubernetes (GKE, EKS, AKS)](#managed-kubernetes-gke-eks-aks)
  - [Helm Installation (Universal)](#helm-installation-universal)
- [Post-Installation Verification](#post-installation-verification)
- [Accessing the Dashboard](#accessing-the-dashboard)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

Before installing Kubarr, ensure you have the following tools installed:

### Required Tools

| Tool | Minimum Version | Purpose | Installation |
|------|----------------|---------|--------------|
| **Docker** | 20.10+ | Container runtime | [docker.com](https://docs.docker.com/get-docker/) |
| **kubectl** | 1.20+ | Kubernetes CLI | [kubernetes.io](https://kubernetes.io/docs/tasks/tools/) |
| **Kubernetes** | 1.20+ | Container orchestration | See [installation methods](#installation-methods) below |

### Optional Tools

| Tool | Minimum Version | Purpose | Installation |
|------|----------------|---------|--------------|
| **Helm** | 3.0+ | Package manager for Kubernetes | [helm.sh](https://helm.sh/docs/intro/install/) |
| **kind** | 0.20+ | Local Kubernetes clusters | [kind.sigs.k8s.io](https://kind.sigs.k8s.io/docs/user/quick-start/) |
| **k3s** | 1.20+ | Lightweight Kubernetes | [k3s.io](https://k3s.io/) |

### System Requirements

- **CPU:** 2+ cores recommended (1 core minimum)
- **Memory:** 4GB RAM minimum (8GB recommended for production)
- **Disk:** 10GB available space
- **OS:** Linux, macOS, or Windows with WSL2

---

## Installation Methods

Choose the installation method that best fits your environment:

### Kind (Local Development)

**Best for:** Local development, testing, and CI/CD pipelines

Kind (Kubernetes in Docker) creates lightweight Kubernetes clusters using Docker containers. This is the recommended approach for local development.

#### Automated Setup

The fastest way to get started with Kind:

```bash
# 1. Clone the repository
git clone https://github.com/yourusername/kubarr.git
cd kubarr

# 2. Run the automated setup script
./scripts/local-k8s-setup.sh
```

This script will:
- Install `kind` and `kubectl` to `./bin/` if not already present
- Create a Kind cluster named `kubarr` with proper port mappings
- Configure kubectl context to point to the new cluster

#### Manual Setup

If you prefer to set up Kind manually:

```bash
# 1. Install kind (if not already installed)
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.24.0/kind-linux-amd64
chmod +x ./kind
sudo mv ./kind /usr/local/bin/kind

# 2. Create a Kind cluster with port mappings
cat <<EOF | kind create cluster --name kubarr --wait 60s --config=-
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  extraPortMappings:
  - containerPort: 30080
    hostPort: 8080
    protocol: TCP
EOF

# 3. Verify the cluster is running
kubectl cluster-info --context kind-kubarr
```

#### Deploy Kubarr to Kind

```bash
# Build and deploy using the automated script
./scripts/deploy.sh

# The script will:
# 1. Build backend and frontend Docker images
# 2. Load images into the Kind cluster
# 3. Apply Kubernetes manifests
# 4. Wait for deployments to be ready
```

---

### Kind (Remote Build Server)

**Best for:** Offloading Docker builds to a more powerful server on your local network

This approach uses Docker contexts to build images on a remote server while keeping the development workflow local.

#### Prerequisites

- A remote Linux server with:
  - Docker installed and running
  - SSH access with key-based authentication
  - Sufficient resources (4GB+ RAM, 2+ CPU cores)
  - Network connectivity on port 6443 (Kubernetes API)

#### One-Time Remote Setup

```bash
# Configure the remote server, Docker context, and Kind cluster
./scripts/remote-server-setup.sh --host <REMOTE_IP> --user <REMOTE_USER>

# Example with custom SSH key
./scripts/remote-server-setup.sh --host 192.168.1.100 --user bmartens --key ~/.ssh/id_ed25519
```

This script will:
1. Verify SSH key-based authentication to the remote server
2. Check that Docker is installed and running on the remote server
3. Create a Docker context named `kubarr-remote` targeting the remote Docker daemon
4. Create a Kind cluster on the remote server
5. Retrieve and merge the remote kubeconfig into `~/.kube/config`

#### Deploy to Remote Kind Cluster

```bash
# Build and deploy to the remote cluster
./scripts/deploy.sh --remote

# This will:
# 1. Use the 'kubarr-remote' Docker context for builds
# 2. Build images on the remote server
# 3. Load images into the remote Kind cluster
# 4. Deploy using kubectl with the remote context
```

#### Manual Remote Deployment

If you prefer step-by-step control:

```bash
# 1. Switch to remote Docker context
docker context use kubarr-remote

# 2. Build images on remote server
docker build -t kubarr-backend:latest -f docker/Dockerfile.backend --build-arg PROFILE=dev-release .
docker build -t kubarr-frontend:latest -f docker/Dockerfile.frontend .

# 3. Load images into remote Kind cluster
kind load docker-image kubarr-backend:latest --name kubarr
kind load docker-image kubarr-frontend:latest --name kubarr

# 4. Deploy to remote cluster
kubectl --context kind-kubarr apply -f k8s/
kubectl --context kind-kubarr rollout restart deployment/kubarr-backend deployment/kubarr-frontend -n kubarr
kubectl --context kind-kubarr rollout status deployment/kubarr-backend deployment/kubarr-frontend -n kubarr --timeout=120s

# 5. Switch back to local Docker context
docker context use default
```

#### Remote Access

```bash
# Start port forwarding from the remote cluster
kubectl --context kind-kubarr port-forward -n kubarr svc/kubarr-frontend 8080:80

# Access the dashboard at http://localhost:8080
```

---

### k3s (Lightweight Kubernetes)

**Best for:** Resource-constrained environments, edge computing, Raspberry Pi, IoT devices

k3s is a lightweight, certified Kubernetes distribution perfect for production workloads on minimal hardware.

#### Install k3s

```bash
# Install k3s on the server
curl -sfL https://get.k3s.io | sh -

# Wait for k3s to be ready
sudo k3s kubectl wait --for=condition=ready node --all --timeout=60s

# Set up kubeconfig for kubectl access
mkdir -p ~/.kube
sudo k3s kubectl config view --raw > ~/.kube/config
chmod 600 ~/.kube/config

# Or export directly
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
```

#### Deploy Kubarr on k3s

##### Option 1: Using Helm (Recommended)

```bash
# 1. Create namespace
kubectl create namespace kubarr

# 2. Install Kubarr with Helm
helm install kubarr ./charts/kubarr -n kubarr

# 3. Wait for pods to be ready
kubectl wait --for=condition=ready pod -l app.kubernetes.io/name=kubarr -n kubarr --timeout=300s

# 4. Access via port-forward
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
```

##### Option 2: Using Kubernetes Manifests

```bash
# 1. Build images (requires Docker on k3s node)
docker build -t kubarr-backend:latest -f docker/Dockerfile.backend --build-arg PROFILE=dev-release .
docker build -t kubarr-frontend:latest -f docker/Dockerfile.frontend .

# 2. Import images to k3s
sudo k3s ctr images import kubarr-backend.tar
sudo k3s ctr images import kubarr-frontend.tar

# 3. Apply manifests
kubectl apply -f k8s/

# 4. Verify deployment
kubectl get pods -n kubarr
```

---

### Managed Kubernetes (GKE, EKS, AKS)

**Best for:** Production deployments, scalability, managed infrastructure

Deploy Kubarr to production-grade managed Kubernetes services.

#### Google Kubernetes Engine (GKE)

```bash
# 1. Create a GKE cluster
gcloud container clusters create kubarr-cluster \
  --zone us-central1-a \
  --num-nodes 3 \
  --machine-type n1-standard-2 \
  --enable-autoscaling \
  --min-nodes 1 \
  --max-nodes 5

# 2. Get credentials
gcloud container clusters get-credentials kubarr-cluster --zone us-central1-a

# 3. Install Kubarr with Helm
helm install kubarr ./charts/kubarr -n kubarr --create-namespace

# 4. Access the dashboard
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
```

#### Amazon Elastic Kubernetes Service (EKS)

```bash
# 1. Create an EKS cluster
eksctl create cluster \
  --name kubarr-cluster \
  --region us-west-2 \
  --nodegroup-name standard-workers \
  --node-type t3.medium \
  --nodes 3 \
  --nodes-min 1 \
  --nodes-max 5 \
  --managed

# 2. Update kubeconfig
aws eks update-kubeconfig --region us-west-2 --name kubarr-cluster

# 3. Install AWS Load Balancer Controller
kubectl apply -k "github.com/aws/eks-charts/stable/aws-load-balancer-controller//crds?ref=master"
helm repo add eks https://aws.github.io/eks-charts
helm install aws-load-balancer-controller eks/aws-load-balancer-controller \
  -n kube-system \
  --set clusterName=kubarr-cluster

# 4. Install Kubarr with Helm
helm install kubarr ./charts/kubarr -n kubarr --create-namespace

# 5. Access the dashboard
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
```

#### Azure Kubernetes Service (AKS)

```bash
# 1. Create a resource group
az group create --name kubarr-rg --location eastus

# 2. Create an AKS cluster
az aks create \
  --resource-group kubarr-rg \
  --name kubarr-cluster \
  --node-count 3 \
  --node-vm-size Standard_D2s_v3 \
  --enable-managed-identity \
  --enable-cluster-autoscaler \
  --min-count 1 \
  --max-count 5

# 3. Get credentials
az aks get-credentials --resource-group kubarr-rg --name kubarr-cluster

# 4. Install Kubarr with Helm
helm install kubarr ./charts/kubarr -n kubarr --create-namespace

```

---

### Helm Installation (Universal)

**Best for:** Any Kubernetes cluster with Helm support

The Helm chart is the most flexible installation method and works on any Kubernetes cluster.

#### Basic Installation

```bash
# 1. Create namespace
kubectl create namespace kubarr

# 2. Install the chart
helm install kubarr ./charts/kubarr -n kubarr

# 3. Wait for pods to be ready
kubectl wait --for=condition=ready pod -l app.kubernetes.io/name=kubarr -n kubarr --timeout=300s
```

#### Custom Values Installation

Create a custom values file:

```yaml
# custom-values.yaml
backend:
  replicaCount: 2
  resources:
    limits:
      cpu: 1000m
      memory: 1Gi
    requests:
      cpu: 200m
      memory: 512Mi
  env:
    - name: KUBARR_LOG_LEVEL
      value: "INFO"
    - name: KUBARR_DEFAULT_NAMESPACE
      value: "media-apps"

frontend:
  replicaCount: 2
  resources:
    limits:
      cpu: 500m
      memory: 512Mi
    requests:
      cpu: 100m
      memory: 256Mi

```

Install with custom values:

```bash
helm install kubarr ./charts/kubarr -n kubarr --create-namespace -f custom-values.yaml
```

#### Helm Chart Parameters

See the [Helm Chart README](../charts/kubarr/README.md) for a complete list of configurable parameters.

---

## Post-Installation Verification

After installation, verify that Kubarr is running correctly:

### 1. Check Pod Status

```bash
# Verify all pods are running
kubectl get pods -n kubarr

# Expected output:
# NAME                              READY   STATUS    RESTARTS   AGE
# kubarr-backend-xxxxxxxxxx-xxxxx   1/1     Running   0          2m
# kubarr-frontend-xxxxxxxxxx-xxxxx  1/1     Running   0          2m
```

All pods should show `1/1` in the READY column and `Running` in the STATUS column.

### 2. Check Service Status

```bash
# Verify services are created
kubectl get svc -n kubarr

# Expected output:
# NAME              TYPE        CLUSTER-IP      EXTERNAL-IP   PORT(S)    AGE
# kubarr-backend    ClusterIP   10.96.xxx.xxx   <none>        8000/TCP   2m
# kubarr-frontend   ClusterIP   10.96.xxx.xxx   <none>        80/TCP     2m
```

### 3. Check Logs

```bash
# Check backend logs
kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-backend --tail=50

# Check frontend logs
kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-frontend --tail=50
```

Look for any error messages or warnings in the logs.

### 4. Test API Health Endpoint

```bash
# Port forward to the backend
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &

# Wait a moment, then test the health endpoint
sleep 2
curl -s http://localhost:8080/api/health

# Expected output:
# {"status":"ok","version":"0.1.0"}
```

### 5. Verify RBAC Permissions

```bash
# Check that the service account has proper permissions
kubectl auth can-i list pods --as=system:serviceaccount:kubarr:kubarr -n kubarr

# Expected output: yes
```

### 6. Test Frontend Access

```bash
# Port forward to the frontend
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80

# Open http://localhost:8080 in your browser
# You should see the Kubarr dashboard login page
```

---

## Accessing the Dashboard

### Port Forwarding (Development)

The simplest way to access Kubarr during development:

```bash
# For Kind (local)
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80

# For Kind (remote)
kubectl --context kind-kubarr port-forward -n kubarr svc/kubarr-frontend 8080:80

# For k3s or managed clusters
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
```

Then visit http://localhost:8080 in your browser.

**Note:** Port forwarding must be restarted after every backend deployment:

```bash
# Kill existing port-forward
pkill -f "port-forward.*kubarr"

# Restart port-forward
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &
```

### NodePort (For Testing)

Expose Kubarr via NodePort for quick testing:

```bash
# Patch the frontend service to use NodePort
kubectl patch svc kubarr-frontend -n kubarr -p '{"spec":{"type":"NodePort"}}'

# Get the NodePort
kubectl get svc kubarr-frontend -n kubarr

# Access via http://<NODE_IP>:<NODE_PORT>
```

---

## Troubleshooting

### Common Issues and Solutions

#### Pods Not Starting

**Symptom:** Pods stuck in `Pending`, `ContainerCreating`, or `CrashLoopBackOff` state

```bash
# Check pod events
kubectl describe pod -n kubarr <pod-name>

# Common causes:
# 1. Insufficient resources
kubectl top nodes

# 2. Image pull errors
kubectl get events -n kubarr --sort-by='.lastTimestamp'

# 3. Volume mount issues
kubectl get pvc -n kubarr
```

**Solutions:**
- For resource constraints: Scale down other workloads or add more nodes
- For image pull errors: Check Docker Hub rate limits, verify image names/tags
- For volume issues: Check storage class and PVC status

#### Port Forward Breaks After Deployment

**Symptom:** `curl http://localhost:8080` returns connection refused after redeploying

**Solution:**
Port forwarding terminates when pods restart. Always restart port forwarding after deployment:

```bash
# Kill existing port-forward processes
pkill -f "port-forward.*kubarr"

# Restart port-forward
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &

# Verify it's working
sleep 2
curl -s http://localhost:8080/api/health
```

#### Remote Docker Context Issues

**Symptom:** `Cannot connect to Docker daemon` or `x509 certificate errors`

**Solutions:**

```bash
# 1. Verify Docker context
docker context ls

# 2. Ensure DOCKER_HOST is unset
unset DOCKER_HOST

# 3. Test remote Docker connection
docker --context kubarr-remote info

# 4. For x509 errors, verify Kind cluster API server address
# The API server must use the remote server's IP, not 0.0.0.0 or 127.0.0.1
kubectl cluster-info

# 5. Verify SSH connectivity
ssh <USER>@<HOST> 'echo OK'
```

#### Kind Cluster Not Accessible

**Symptom:** `Unable to connect to the server`

**Solutions:**

```bash
# 1. Check if the cluster exists
kind get clusters

# 2. Verify kubectl context
kubectl config get-contexts

# 3. Set the correct context
kubectl config use-context kind-kubarr

# 4. If cluster is gone, recreate it
kind delete cluster --name kubarr
./scripts/local-k8s-setup.sh
```

#### Database Connection Errors

**Symptom:** Backend logs show database connection failures

**Solutions:**

```bash
# 1. Check if database pod is running (if using PostgreSQL)
kubectl get pods -n kubarr | grep postgres

# 2. Verify database credentials
kubectl get secret -n kubarr kubarr-db-secret -o yaml

# 3. For SQLite (development), check volume mounts
kubectl describe pod -n kubarr <backend-pod-name> | grep -A 5 Mounts

# 4. Check environment variables
kubectl exec -n kubarr <backend-pod-name> -- env | grep DATABASE
```

#### RBAC Permission Denied

**Symptom:** Dashboard shows errors like "forbidden: User cannot list pods"

**Solutions:**

```bash
# 1. Verify service account exists
kubectl get sa -n kubarr kubarr

# 2. Check ClusterRole and ClusterRoleBinding
kubectl get clusterrole kubarr
kubectl get clusterrolebinding kubarr

# 3. Verify permissions
kubectl auth can-i list pods --as=system:serviceaccount:kubarr:kubarr

# 4. Reapply RBAC manifests
kubectl apply -f k8s/rbac.yaml
```

#### High Memory Usage

**Symptom:** Pods being OOMKilled or high memory consumption

**Solutions:**

```bash
# 1. Check current resource usage
kubectl top pods -n kubarr

# 2. Increase memory limits
helm upgrade kubarr ./charts/kubarr -n kubarr \
  --set backend.resources.limits.memory=1Gi \
  --set frontend.resources.limits.memory=512Mi

# 3. Add resource requests for better scheduling
helm upgrade kubarr ./charts/kubarr -n kubarr \
  --set backend.resources.requests.memory=512Mi \
  --set backend.resources.requests.cpu=200m
```

#### TLS Certificate Issues

**Symptom:** Browser shows "Not Secure" or certificate errors

**Solutions:**

```bash
# 1. Check cert-manager is running
kubectl get pods -n cert-manager

# 2. Verify certificate status
kubectl get certificate -n kubarr

# 3. Check certificate request
kubectl describe certificaterequest -n kubarr

# 4. Review cert-manager logs
kubectl logs -n cert-manager -l app=cert-manager

# 5. Ensure ClusterIssuer is ready
kubectl get clusterissuer letsencrypt-prod -o yaml
```

### Getting Help

If you encounter issues not covered here:

1. **Check Logs:**
   ```bash
   kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-backend --tail=100
   kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-frontend --tail=100
   ```

2. **Describe Resources:**
   ```bash
   kubectl describe pod -n kubarr <pod-name>
   kubectl describe svc -n kubarr
   kubectl describe svc -n kubarr kubarr-backend
   ```

3. **Check Events:**
   ```bash
   kubectl get events -n kubarr --sort-by='.lastTimestamp'
   ```

4. **Open an Issue:**
   - Visit [GitHub Issues](https://github.com/yourusername/kubarr/issues)
   - Include logs, resource descriptions, and cluster information

5. **Community Support:**
   - [GitHub Discussions](https://github.com/yourusername/kubarr/discussions)
   - Check existing issues for similar problems

---

## Next Steps

After successful installation:

1. **Configure Kubarr:** See [Configuration Reference](./configuration.md) for customization options
2. **Learn the UI:** Read the [User Guide](./user-guide.md) to understand dashboard features
3. **Deploy Applications:** Use Kubarr to deploy and manage your Kubernetes workloads
4. **Set Up Monitoring:** Configure alerts and notifications for your cluster
5. **Secure Your Installation:** Review [SECURITY.md](../code/backend/SECURITY.md) for best practices

---

**Note:** Always review security settings and RBAC permissions before deploying Kubarr to production environments. The default configuration is intended for development and should be hardened for production use.
