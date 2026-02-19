# Kubarr Architecture

This document describes Kubarr's system architecture, component relationships, and data model.

## Component Overview

Kubarr consists of four main components:

| Component | Technology | Role |
|-----------|-----------|------|
| **Frontend** | React, TypeScript, Tailwind CSS, served by BusyBox httpd | Single-page application; provides the dashboard UI |
| **Backend** | Rust, Axum, SeaORM, kube-rs | REST API server; orchestrates Kubernetes and database operations |
| **Database** | PostgreSQL (production) / SQLite (development) | Stores users, sessions, roles, audit logs, and settings |
| **Kubernetes** | kube-rs client, Helm | Deploys and manages media application workloads |

### System Diagram

```mermaid
graph TB
    Browser["Browser"]
    Frontend["Frontend\n(React SPA / BusyBox)"]
    Backend["Backend\n(Rust / Axum)"]
    DB["Database\n(PostgreSQL)"]
    K8s["Kubernetes API"]
    Helm["Helm / OCI Charts"]
    Apps["Media App Pods\n(per-namespace)"]

    Browser -->|"HTTP :80"| Frontend
    Browser -->|"REST /api/*"| Backend
    Backend -->|"SeaORM"| DB
    Backend -->|"kube-rs"| K8s
    Backend -->|"helm install/upgrade"| Helm
    K8s --> Apps
    Helm --> Apps
```

---

## Request Flow

A typical authenticated API request:

```mermaid
sequenceDiagram
    participant B as Browser
    participant F as Frontend (BusyBox)
    participant A as Backend (Axum)
    participant D as Database
    participant K as Kubernetes API

    B->>F: GET /
    F-->>B: index.html + JS bundle

    B->>A: GET /api/apps/catalog<br/>[Authorization: Bearer <token>]
    A->>D: Validate session token
    D-->>A: Session valid, user resolved
    A->>K: List namespaces / deployments
    K-->>A: Resource list
    A-->>B: JSON response
```

---

## Authentication Flow

Kubarr uses session tokens stored in the database. JWT-style tokens are issued at login and validated on every request.

```mermaid
sequenceDiagram
    participant B as Browser
    participant A as Backend
    participant D as Database

    B->>A: POST /api/auth/login<br/>{username, password}
    A->>D: Lookup user by username
    D-->>A: User record (hashed_password, is_active, is_approved)
    A->>A: Verify bcrypt hash
    A->>D: INSERT INTO sessions (id, user_id, expires_at, ...)
    D-->>A: Session created
    A-->>B: {token: "<session-id>", ...}

    Note over B,A: Subsequent requests

    B->>A: GET /api/... [Authorization: Bearer <session-id>]
    A->>D: SELECT * FROM sessions WHERE id = ?<br/>AND expires_at > NOW() AND is_revoked = false
    D-->>A: Session + user row
    A->>A: Check role permissions
    A-->>B: Protected resource
```

**2FA (TOTP) flow:** After password verification, if `totp_enabled = true` the server creates a `pending_2fa_challenge` record and returns a `requires_2fa` response. The client then submits the TOTP code to `/api/auth/2fa/verify` which validates the TOTP secret and completes session creation.

---

## App Deployment Flow

When a user installs a media application from the catalog:

```mermaid
sequenceDiagram
    participant U as User (Browser)
    participant A as Backend
    participant H as Helm
    participant K as Kubernetes

    U->>A: POST /api/apps/install<br/>{app_name, ...}
    A->>A: Check permission (AppsInstall)
    A->>A: Lookup AppConfig from catalog<br/>(parsed from Helm Chart.yaml)
    A->>H: helm install <app> <chart><br/>--namespace <app> --create-namespace
    H->>K: Create Namespace: <app>
    H->>K: Create Deployment, Service,<br/>PersistentVolumeClaim, NetworkPolicy
    K-->>H: Resources scheduled
    H-->>A: Helm release created
    A->>A: Log AuditAction::AppInstalled
    A-->>U: {state: "installing", ...}

    Note over U,K: User polls /api/apps/<app>/status

    K->>K: Pull container image
    K->>K: Schedule pod on node
    U->>A: GET /api/apps/<app>/status
    A->>K: Check namespace health / pod readiness
    K-->>A: All pods Ready
    A-->>U: {state: "installed", message: "Running"}
```

---

## Data Model Overview

The following ER diagram covers the core entities. Notification and VPN tables are omitted for brevity.

```mermaid
erDiagram
    users {
        bigint id PK
        string username
        string email
        string hashed_password
        boolean is_active
        boolean is_approved
        string totp_secret
        boolean totp_enabled
        timestamp created_at
        timestamp updated_at
    }

    sessions {
        string id PK
        bigint user_id FK
        string user_agent
        string ip_address
        timestamp created_at
        timestamp expires_at
        timestamp last_accessed_at
        boolean is_revoked
    }

    roles {
        bigint id PK
        string name
        string description
    }

    user_roles {
        bigint user_id FK
        bigint role_id FK
    }

    role_permissions {
        bigint id PK
        bigint role_id FK
        string permission
    }

    role_app_permissions {
        bigint id PK
        bigint role_id FK
        string app_name
        string permission
    }

    audit_logs {
        bigint id PK
        string action
        string resource_type
        string resource_id
        bigint user_id FK
        string username
        jsonb details
        timestamp created_at
    }

    system_settings {
        string key PK
        string value
        string description
        timestamp updated_at
    }

    invites {
        string token PK
        bigint created_by FK
        string email
        boolean used
        timestamp expires_at
    }

    oauth_accounts {
        bigint id PK
        bigint user_id FK
        bigint provider_id FK
        string provider_user_id
    }

    oauth_providers {
        bigint id PK
        string name
        string client_id
        string client_secret
        boolean enabled
    }

    users ||--o{ sessions : "has"
    users ||--o{ user_roles : "assigned"
    roles ||--o{ user_roles : "granted to"
    roles ||--o{ role_permissions : "has"
    roles ||--o{ role_app_permissions : "has"
    users ||--o{ audit_logs : "generates"
    users ||--o{ oauth_accounts : "linked"
    oauth_providers ||--o{ oauth_accounts : "issues"
    users ||--o{ invites : "creates"
```

---

## Security Architecture

- **Namespace isolation** — each installed application runs in its own Kubernetes namespace, preventing lateral movement between apps.
- **RBAC** — every API endpoint requires a specific permission (e.g. `AppsInstall`, `AppsDelete`). Permissions are attached to roles; roles are assigned to users.
- **Session invalidation** — sessions can be revoked individually (`is_revoked = true`) or expired via `expires_at`. The backend validates both on every request.
- **2FA** — optional TOTP second factor enforced before session creation.
- **Audit log** — all state-changing operations (install, delete, login, settings change) are recorded in `audit_logs` with user, timestamp, and details.
- **Network policies** — Helm charts deploy Kubernetes `NetworkPolicy` resources restricting intra-cluster traffic.
