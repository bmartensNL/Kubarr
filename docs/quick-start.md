# Kubarr Quick Start Guide

Get Kubarr running on your local Kubernetes cluster in under 15 minutes.

## Prerequisites Check

Before starting, verify you have Docker installed:

```bash
# Check Docker is installed and running
docker --version
# Expected: Docker version 20.10+ or higher

# Verify Docker daemon is running
docker ps
# Should return a list of containers (or empty list if none running)
```

**System Requirements:**
- **Docker:** 20.10+ (required)
- **OS:** Linux, macOS, or Windows with WSL2
- **Memory:** 4GB RAM minimum
- **Disk:** 10GB available space

**Note:** You do NOT need to install `kubectl` or `kind` manually - the setup script will install them for you.

---

## Step 1: Clone the Repository

```bash
# Clone Kubarr
git clone https://github.com/yourusername/kubarr.git
cd kubarr
```

---

## Step 2: Set Up Local Kubernetes Cluster

Run the automated setup script to create a Kind (Kubernetes in Docker) cluster:

```bash
./scripts/local-k8s-setup.sh
```

**What this does:**
- ✅ Installs `kind` and `kubectl` to `./bin/` (if not already present)
- ✅ Creates a Kind cluster named `kubarr` with port mappings
- ✅ Configures kubectl to access the cluster

**Expected output:**
```
=== Kubarr Local K8s Setup ===
Installing kind to ./bin/kind...
Installing kubectl to ./bin/kubectl...
Creating kind cluster 'kubarr' with port mappings...
...
Kubernetes control plane is running at https://127.0.0.1:xxxxx

=== Setup Complete ===

Run ./scripts/deploy.sh to build and deploy Kubarr
```

⏱️ **Time:** ~2-3 minutes

---

## Step 3: Deploy Kubarr

Build Docker images and deploy to your cluster:

```bash
./scripts/deploy.sh
```

**What this does:**
- 🔨 Builds backend Docker image (Rust/Axum API server)
- 🔨 Builds frontend Docker image (React dashboard)
- 📦 Loads images into the Kind cluster
- 🚀 Deploys Kubarr to Kubernetes
- ⏳ Waits for deployments to be ready

**Expected output:**
```
=== Kubarr Deploy ===
Building backend...
Building frontend...
Loading images into kind...
Applying Kubernetes manifests...
Restarting deployments...
Waiting for deployments...
...
deployment "kubarr-backend" successfully rolled out
deployment "kubarr-frontend" successfully rolled out

=== Deploy Complete ===

Access Kubarr at: http://localhost:8080
```

⏱️ **Time:** ~5-10 minutes (depending on Docker build performance)

---

## Step 4: Access the Dashboard

Start port forwarding to access the Kubarr dashboard:

```bash
# Port forward to the frontend service
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80
```

**Keep this terminal open** while using Kubarr. The port-forward process must remain running.

Now open your browser and navigate to:

```
http://localhost:8080
```

You should see the Kubarr dashboard login page.

⏱️ **Time:** ~30 seconds

---

## Step 5: Complete Initial Setup

On first access, Kubarr will guide you through initial setup:

### 5.1 Create Admin Account

1. The setup wizard will prompt you to create an admin account
2. Enter a username (e.g., `admin`)
3. Set a strong password
4. (Optional) Configure two-factor authentication

### 5.2 Configure Cluster Connection

Kubarr automatically detects it's running inside a Kubernetes cluster and configures access using the service account.

**Verify cluster connection:**
- The dashboard should display cluster information (number of nodes, Kubernetes version)
- You should see existing namespaces listed in the sidebar

### 5.3 Set Default Namespace

Choose a default namespace for deploying applications:

1. Navigate to **Settings** → **Cluster Configuration**
2. Set **Default Namespace** (e.g., `default` or `kubarr-apps`)
3. Click **Save**

⏱️ **Time:** ~2-3 minutes

---

## Step 6: Deploy Your First Application

Let's deploy a simple nginx application to verify everything works:

### Option 1: Using the Application Catalog (Recommended)

1. Navigate to **Applications** → **Catalog**
2. Search for "nginx"
3. Click **Deploy**
4. Configure the deployment:
   - **Name:** `my-nginx`
   - **Namespace:** `default` (or your chosen namespace)
   - **Replicas:** `1`
5. Click **Deploy Application**
6. Wait for the deployment to be ready (~30 seconds)

### Option 2: Using YAML Editor

1. Navigate to **Applications** → **Deploy from YAML**
2. Paste the following manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-nginx
  namespace: default
spec:
  replicas: 1
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: nginx
    spec:
      containers:
      - name: nginx
        image: nginx:latest
        ports:
        - containerPort: 80
---
apiVersion: v1
kind: Service
metadata:
  name: my-nginx
  namespace: default
spec:
  selector:
    app: nginx
  ports:
  - port: 80
    targetPort: 80
  type: ClusterIP
```

3. Click **Deploy**
4. Monitor the deployment status in real-time

### Verify the Deployment

1. Navigate to **Applications** → **Deployments**
2. Find `my-nginx` in the list
3. Click to view details:
   - **Status:** Should show `1/1` pods ready
   - **Health:** Green checkmark
   - **Resource Usage:** CPU and memory metrics

### Access Your Application

```bash
# Port forward to the nginx service
kubectl port-forward -n default svc/my-nginx 8081:80

# In another terminal, test the application
curl http://localhost:8081
# Expected: HTML response from nginx default page
```

⏱️ **Time:** ~2-3 minutes

---

## 🎉 Success!

You now have Kubarr running and have deployed your first application. Here's what you've accomplished:

✅ Created a local Kubernetes cluster with Kind
✅ Deployed Kubarr backend and frontend
✅ Accessed the Kubarr dashboard
✅ Completed initial setup
✅ Deployed and verified a sample application

**Total Time:** ~15 minutes

---

## Next Steps

### Explore Kubarr Features

- **📊 Monitoring:** View real-time resource usage and pod health
- **⚙️ Configuration:** Manage ConfigMaps and Secrets through the UI
- **📦 Helm Charts:** Deploy complex applications using Helm
- **🔄 Auto-Scaling:** Configure horizontal pod autoscaling
- **📝 YAML Editor:** Edit Kubernetes manifests with validation
- **🔔 Alerts:** Set up notifications for deployment events

### Learn More

- **[User Guide](./user-guide.md)** - Detailed feature documentation
- **[Configuration Reference](./configuration.md)** - Customize Kubarr settings
- **[Installation Guide](./installation.md)** - Deploy to production clusters (GKE, EKS, AKS)
- **[API Documentation](./api.md)** - Integrate with Kubarr programmatically

### Deploy Real Applications

Try deploying more complex applications:

- **PostgreSQL Database:** Persistent storage with StatefulSets
- **Redis Cache:** In-memory data store
- **Monitoring Stack:** Prometheus + Grafana
- **Ingress Controller:** Expose services to the internet

---

## Common Tasks

### Restart Port Forwarding

Port forwarding breaks when pods restart. To restart:

```bash
# Kill existing port-forward processes
pkill -f "port-forward.*kubarr"

# Restart port-forward to frontend
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &

# Verify access
curl -s http://localhost:8080
```

### Rebuild and Redeploy

After making code changes:

```bash
# Rebuild and redeploy
./scripts/deploy.sh

# Wait for rollout to complete (automatic)

# Restart port-forward (required after every deployment)
pkill -f "port-forward.*kubarr"
kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &
```

### Check Pod Status

```bash
# View all Kubarr pods
kubectl get pods -n kubarr

# Check pod logs
kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-backend --tail=50
kubectl logs -n kubarr -l app.kubernetes.io/name=kubarr-frontend --tail=50
```

### Test Backend Health

```bash
# Port forward to backend
kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000 &

# Check health endpoint
curl -s http://localhost:8080/api/health
# Expected: {"status":"ok","version":"0.1.0"}
```

### Delete Everything

To completely remove Kubarr and the Kind cluster:

```bash
# Delete the Kind cluster (removes everything)
kind delete cluster --name kubarr

# Clean up local binaries (optional)
rm -rf ./bin
```

---

## Troubleshooting

### Port 8080 Already in Use

```bash
# Find the process using port 8080
lsof -i :8080

# Kill the process
kill -9 <PID>

# Or use a different port
kubectl port-forward -n kubarr svc/kubarr-frontend 8081:80
# Access at http://localhost:8081
```

### Docker Build is Slow

Docker builds can take 5-10 minutes on slower machines. To speed up:

1. **Use a remote build server** (if available on your network):
   ```bash
   # One-time setup
   ./scripts/remote-server-setup.sh --host <REMOTE_IP> --user <USER>

   # Deploy using remote server
   ./scripts/deploy.sh --remote
   ```

2. **Enable Docker BuildKit:**
   ```bash
   export DOCKER_BUILDKIT=1
   ./scripts/deploy.sh
   ```

### Pods Not Starting

```bash
# Check pod status
kubectl get pods -n kubarr

# Describe pod for details
kubectl describe pod -n kubarr <pod-name>

# Check events
kubectl get events -n kubarr --sort-by='.lastTimestamp'
```

### Cannot Access Dashboard

1. **Verify port-forward is running:**
   ```bash
   ps aux | grep port-forward
   ```

2. **Restart port-forward:**
   ```bash
   pkill -f "port-forward.*kubarr"
   kubectl port-forward -n kubarr svc/kubarr-frontend 8080:80 &
   ```

3. **Check frontend pod is ready:**
   ```bash
   kubectl get pods -n kubarr
   # Both backend and frontend should show 1/1 READY
   ```

### Need More Help?

- **Detailed Troubleshooting:** See [Installation Guide - Troubleshooting](./installation.md#troubleshooting)
- **GitHub Issues:** [Report a bug](https://github.com/yourusername/kubarr/issues)
- **GitHub Discussions:** [Ask questions](https://github.com/yourusername/kubarr/discussions)

---

## Remote Development (Advanced)

If you have a more powerful server on your local network, you can offload Docker builds:

### One-Time Remote Setup

```bash
./scripts/remote-server-setup.sh --host <REMOTE_IP> --user <REMOTE_USER>

# Example:
./scripts/remote-server-setup.sh --host 192.168.1.100 --user bmartens
```

**Prerequisites:**
- Remote Linux server with Docker installed
- SSH key-based authentication configured
- Port 6443 accessible (Kubernetes API)

### Deploy to Remote Cluster

```bash
# Build and deploy using remote server
./scripts/deploy.sh --remote

# Access via port-forward (same as local)
kubectl --context kind-kubarr port-forward -n kubarr svc/kubarr-frontend 8080:80
```

**Benefits:**
- ⚡ Faster builds on powerful hardware
- 💻 Frees up local resources
- 🔄 Same development workflow

See [CLAUDE.md](../CLAUDE.md#build-and-deploy-backend-remote) for detailed remote workflow documentation.

---

**Ready to dive deeper?** Check out the [User Guide](./user-guide.md) to explore all of Kubarr's features.
