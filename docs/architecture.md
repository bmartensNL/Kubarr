# Architecture

Kubarr is a Kubernetes-native dashboard for managing media server applications. This document covers the system design, component interactions, and data model.

## Component Overview

Kubarr consists of four main components:

| Component | Technology | Role |
|-----------|-----------|------|
| **Frontend** | React, TypeScript, Tailwind CSS | Single-page application served as static files via BusyBox/nginx |
| **Backend** | Rust, Axum, SeaORM, kube-rs | REST API server with Kubernetes client integration |
| **Database** | PostgreSQL (production), SQLite (development) | Application state, sessions, users, audit logs |
| **Kubernetes** | k3s, Kind, EKS, GKE, AKS (1.20+) | Orchestration platform for Kubarr itself and managed applications |

### Backend (Rust/Axum)

The backend is a stateless API server that:

- Handles authentication (JWT-based) and session management
- Proxies requests to the Kubernetes API via `kube-rs`
- Installs and manages media applications via Helm
- Stores application state in PostgreSQL via SeaORM
- Emits structured audit logs for all privileged operations
- Exposes REST endpoints under `/api/`

### Frontend (React/BusyBox)

The frontend is a compiled React SPA served as static files. In the Docker image, BusyBox httpd (or nginx) serves the files. The SPA communicates exclusively with the backend API — it has no direct access to Kubernetes or the database.

### Database (PostgreSQL)

PostgreSQL stores:

- User accounts, roles, and permissions
- Active sessions (JWT refresh tokens)
- Application catalog metadata
- Audit log entries
- System configuration and notification settings

Migrations run automatically at backend startup via SeaORM migrations. No manual SQL steps are required.

### Kubernetes Integration

The backend uses an in-cluster `ServiceAccount` with a `ClusterRole` to:

- List and watch namespaces, pods, deployments, services
- Create and delete namespaces for managed applications
- Install, upgrade, and uninstall Helm releases
- Read pod logs and resource metrics

---

## System Diagram

```mermaid
graph TD
    Browser["Browser\n(User)"]
    FE["Frontend\n(React SPA / BusyBox)"]
    BE["Backend\n(Rust / Axum)"]
    DB["Database\n(PostgreSQL)"]
    K8S["Kubernetes API"]
    Helm["Helm\n(OCI registry)"]
    Apps["Media App Pods\n(Sonarr, Radarr, …)"]

    Browser -->|"HTTP GET /\nStatic assets"| FE
    Browser -->|"HTTP /api/*\nREST calls"| BE
    FE -->|"Proxied API calls"| BE
    BE -->|"SeaORM queries"| DB
    BE -->|"kube-rs client"| K8S
    BE -->|"helm install/upgrade"| Helm
    K8S -->|"Schedules & manages"| Apps
    Helm -->|"Pulls charts from"| GHCR["ghcr.io OCI registry"]
```

---

## Request Flow

A typical API request flows as follows:

```mermaid
sequenceDiagram
    participant B as Browser
    participant FE as Frontend
    participant BE as Backend
    participant DB as PostgreSQL
    participant K8S as Kubernetes API

    B->>FE: HTTP GET / (static asset)
    FE-->>B: React SPA (HTML/JS/CSS)

    B->>BE: POST /api/auth/login
    BE->>DB: Verify credentials, load user/roles
    DB-->>BE: User record + hashed password
    BE-->>B: JWT access token + refresh token (set-cookie)

    B->>BE: GET /api/apps (Authorization: Bearer <token>)
    BE->>DB: Validate session, load permissions
    BE->>K8S: List Helm releases / namespaces
    K8S-->>BE: Release list
    BE-->>B: JSON response
```

---

## Authentication Flow

Kubarr uses JWT-based authentication with refresh tokens stored server-side.

```mermaid
sequenceDiagram
    participant C as Client
    participant BE as Backend
    participant DB as PostgreSQL

    C->>BE: POST /api/auth/login {username, password}
    BE->>DB: Look up user by username
    DB-->>BE: User record (hashed password, roles)
    BE->>BE: Verify bcrypt hash
    BE->>DB: Create Session record (refresh token hash, expiry)
    BE-->>C: { access_token, refresh_token } (+ Set-Cookie)

    Note over C,BE: Subsequent requests

    C->>BE: GET /api/* (Bearer access_token)
    BE->>BE: Validate JWT signature & expiry
    BE->>DB: Load user roles & permissions
    BE-->>C: Protected resource

    Note over C,BE: Token refresh

    C->>BE: POST /api/auth/refresh (refresh_token)
    BE->>DB: Verify Session record (not expired, not revoked)
    BE-->>C: New access_token
```

**Key properties:**

- Access tokens are short-lived (default: 1 hour)
- Refresh tokens are stored as hashed values in the `sessions` table; the raw token is never persisted
- Logging out deletes the session record, invalidating the refresh token immediately
- 2FA (TOTP) challenges are tracked in the `pending_2fa_challenges` table before the session is created

---

## App Deployment Flow

When a user installs a media application from the catalog:

```mermaid
sequenceDiagram
    participant U as User (Browser)
    participant BE as Backend
    participant K8S as Kubernetes API
    participant Helm as Helm / OCI Registry

    U->>BE: POST /api/apps/install {app: "sonarr", values: {...}}
    BE->>BE: Validate request & permissions
    BE->>K8S: Create namespace "sonarr" (if not exists)
    BE->>K8S: Apply NetworkPolicy (isolate namespace)
    BE->>Helm: helm install sonarr oci://ghcr.io/.../sonarr --namespace sonarr
    Helm->>K8S: Create Deployment, Service, PVC, ConfigMap
    K8S-->>BE: Resources created
    BE->>DB: Record app install in audit_log
    BE-->>U: { status: "installing", release: "sonarr" }

    Note over K8S: Kubernetes schedules pods

    U->>BE: GET /api/apps/sonarr/status
    BE->>K8S: Get pod status for namespace "sonarr"
    K8S-->>BE: Pod phase (Running / Pending / …)
    BE-->>U: { status: "running", pods: [...] }
```

---

## Data Model Overview

```mermaid
erDiagram
    users {
        uuid id PK
        string username
        string email
        string password_hash
        bool is_active
        timestamp created_at
    }
    sessions {
        uuid id PK
        uuid user_id FK
        string refresh_token_hash
        timestamp expires_at
        timestamp created_at
    }
    roles {
        uuid id PK
        string name
        string description
    }
    user_roles {
        uuid user_id FK
        uuid role_id FK
    }
    role_permissions {
        uuid id PK
        uuid role_id FK
        string resource
        string action
    }
    role_app_permissions {
        uuid id PK
        uuid role_id FK
        string app_namespace
        string action
    }
    audit_log {
        uuid id PK
        uuid user_id FK
        string action
        string resource
        jsonb details
        timestamp created_at
    }
    invites {
        uuid id PK
        string token_hash
        uuid created_by FK
        timestamp expires_at
        bool used
    }
    oauth_accounts {
        uuid id PK
        uuid user_id FK
        uuid provider_id FK
        string external_id
    }
    oauth_providers {
        uuid id PK
        string name
        string client_id
        string issuer_url
    }
    pending_2fa_challenges {
        uuid id PK
        uuid user_id FK
        string totp_secret
        timestamp expires_at
    }
    notification_channels {
        uuid id PK
        string name
        string type
        jsonb config
    }

    users ||--o{ sessions : "has"
    users ||--o{ user_roles : "assigned"
    roles ||--o{ user_roles : "grants"
    roles ||--o{ role_permissions : "has"
    roles ||--o{ role_app_permissions : "has"
    users ||--o{ audit_log : "generates"
    users ||--o{ oauth_accounts : "links"
    oauth_providers ||--o{ oauth_accounts : "authenticates"
    users ||--o{ pending_2fa_challenges : "challenges"
```

### Key Entities

| Entity | Purpose |
|--------|---------|
| `users` | Human users with credentials |
| `sessions` | Active login sessions; stores hashed refresh tokens |
| `roles` | Named permission groups (e.g., Admin, Viewer) |
| `role_permissions` | Fine-grained resource/action pairs per role |
| `role_app_permissions` | Per-namespace application access per role |
| `audit_log` | Immutable record of all privileged actions |
| `invites` | Time-limited invite tokens for user registration |
| `oauth_accounts` | Links users to external OAuth2 providers |
| `pending_2fa_challenges` | Temporary TOTP challenges before session creation |
| `notification_channels` | Configured alert destinations (email, webhook, etc.) |

---

## Security Architecture

- **Namespace isolation**: Each managed application runs in its own Kubernetes namespace with `NetworkPolicy` rules preventing cross-namespace communication by default.
- **RBAC**: The backend enforces role-based access at the API level. Role permissions are loaded per-request from the database.
- **Secret management**: Sensitive values (JWT signing keys, database URLs) are stored as Kubernetes Secrets and mounted as environment variables — never in the database or in Helm `values.yaml`.
- **Non-root containers**: Both frontend and backend containers run as UID 1000 with `allowPrivilegeEscalation: false`.

## See Also

- [Configuration Reference](configuration.md) — Environment variables and Helm values
- [API Documentation](api.md) — REST API reference
- [ADR: Storage Model](adr/storage-model-architecture.md) — Architectural decision records
