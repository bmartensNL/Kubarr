# ADR: Storage Model Architecture for Kubarr

**Status:** Proposed
**Date:** 2026-01-29
**Authors:** Auto-Claude
**Deciders:** Kubarr maintainers

## Context

Kubarr is a Kubernetes-native homelab management platform designed for single-pod deployment serving 1-5 users. The backend is built with Rust/Axum and currently relies on three storage layers: a PostgreSQL relational database, in-memory caches, and file-system volumes. This ADR documents the current architecture, identifies issues, and evaluates alternative storage models.

### Decision Drivers

- **Operational simplicity** - Homelab operators, not SREs, manage this system
- **Resource efficiency** - Minimize memory and CPU overhead on constrained hardware
- **Data durability** - Protect against data loss without complex backup infrastructure
- **Single-pod architecture** - No horizontal scaling; one backend pod serves all traffic
- **Migration feasibility** - Any change must handle 22 existing entity models and 23 migrations

---

## Current Architecture

### 1. PostgreSQL Database (CloudNativePG 1.28)

**Setup:** PostgreSQL is managed by the CloudNativePG (CNPG) operator v1.28 running in Kubernetes. The backend connects via SeaORM 1.1 with both `sqlx-postgres` and `sqlx-sqlite` feature flags compiled in `Cargo.toml`.

**Connection Management** (`code/backend/src/application/database.rs`):

```rust
let mut opts = ConnectOptions::new(database_url);
opts.max_connections(10)
    .min_connections(1)
    .connect_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(600))
    .sqlx_logging(false);
```

- **Pool size:** Hardcoded at 10 max / 1 min connections - not configurable via environment variables
- **Timeouts:** 30s connect, 600s idle
- **SQL logging:** Disabled (no query-level observability)
- **Retry logic:** None on initial connection failure; `try_connect()` provides graceful degradation with a 5s timeout during bootstrap
- **Migrations:** Run automatically on connect via `Migrator::up(&db, None)`

**Entity Models** (`code/backend/src/models/`): 22 entities covering:

| Category | Models |
|----------|--------|
| Auth & Users | `user`, `session`, `role`, `user_role`, `role_permission`, `role_app_permission`, `invite`, `pending_2fa_challenge` |
| OAuth | `oauth_account`, `oauth_provider` |
| Notifications | `notification_channel`, `notification_event`, `notification_log`, `user_notification`, `user_notification_pref` |
| System | `system_setting`, `server_config`, `bootstrap_status`, `audit_log` |
| VPN | `vpn_provider`, `app_vpn_config` |

All models use standard SeaORM patterns: `DeriveEntityModel` with `Serialize`/`Deserialize`, `i64` primary keys (PostgreSQL `bigint`), and `DateTimeUtc` timestamps.

**Migrations** (`code/backend/src/migrations/`): 23 ordered migrations (including a seed-defaults migration) covering schema creation from `m20260127_000001` through `m20260130_000002`.

**CNPG RBAC** (`charts/kubarr/values.yaml`):

```yaml
- apiGroups: ["postgresql.cnpg.io"]
  resources: ["clusters", "backups", "scheduledbackups", "poolers"]
  verbs: ["get", "list", "watch", "create", "update", "delete", "patch"]
```

The backend has full CRUD access to CNPG resources, enabling programmatic database cluster management from within the application.

### 2. In-Memory Caching (`code/backend/src/application/state.rs`)

Two cache implementations exist in `AppState`:

#### EndpointCache

Caches Kubernetes service endpoint lookups to avoid K8s API calls on every proxy request.

```rust
pub struct EndpointCache {
    cache: Arc<RwLock<HashMap<String, CachedEndpoint>>>,
    ttl: Duration, // 60 seconds
}
```

- **TTL:** 60 seconds, checked on read only (lazy expiration)
- **Eviction:** None - expired entries remain in memory until overwritten by a new `set()` call
- **Size limit:** None - unbounded growth potential
- **Persistence:** Lost on pod restart

#### NetworkMetricsCache

Stores cumulative network counters and computes sliding-window rate averages for dashboard display.

```rust
pub struct NetworkMetricsCache {
    cache: Arc<RwLock<HashMap<String, CachedNetworkMetrics>>>,
    max_age: Duration, // 5 minutes
}
```

- **Staleness:** 5-minute max age, checked on read
- **Rate calculation:** Sliding window of 5 samples with EMA smoothing
- **Eviction:** None - stale entries remain in the HashMap
- **Size limit:** None - one entry per namespace, practical bound is cluster size

#### Additional Shared State

`AppState` also holds several `Arc<RwLock<T>>` wrappers:

- `SharedDbConn` - Optional database connection (deferred until PostgreSQL is available)
- `SharedK8sClient` - Kubernetes client
- `SharedCatalog` - Application catalog
- `NetworkMetricsBroadcast` / `BootstrapBroadcast` - Tokio broadcast channels for WebSocket clients

### 3. File-System Storage (`code/backend/src/endpoints/storage.rs`)

File browsing and download endpoints serve media from a configurable storage path.

- **Configuration:** Storage root from `system_settings` table or `KUBARR_STORAGE_PATH` env var (default: `/data`)
- **Volume type:** hostPath volumes in Kubernetes (no CSI, no PVC abstraction)
- **Security:** Path traversal prevention via canonicalization and base-path containment checks
- **Protected folders:** `downloads`, `media` directories cannot be deleted
- **File operations:** Browse, get info, create directory, delete (empty dirs/files only), stream download
- **Disk stats:** Uses `df -B1` system command for storage usage reporting
- **Permissions:** Role-based (StorageView, StorageWrite, StorageDelete, StorageDownload)

### 4. Session & Authentication Layer (`code/backend/src/middleware/auth.rs`)

Every authenticated request performs:

1. Extract JWT session token from cookies (supports indexed multi-session: `kubarr_session_0`, `kubarr_session_1`, ...)
2. Decode and validate JWT claims
3. **Database lookup:** `Session::find_by_id(&claims.sid).one(&db)` - hits PostgreSQL on every request
4. Validate session is not revoked and not expired
5. **Database lookup:** `User::find_by_id(session.user_id)` with active/approved filters
6. **Fire-and-forget update:** `session.last_accessed_at` written asynchronously
7. **Database lookups:** Fetch all user roles, role permissions, and app permissions (3+ queries)

**No caching exists at the auth layer.** Every authenticated request triggers 4+ database queries. For a homelab with 1-5 users, this is functionally acceptable but architecturally wasteful.

### 5. Key-Value Configuration (`code/backend/src/models/system_setting.rs`)

System settings use a string-keyed key-value pattern:

```rust
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    pub updated_at: DateTimeUtc,
}
```

Used for dynamic configuration (storage path, OAuth2 settings, JWT keys) without requiring schema migrations.

---

## Identified Issues

### Performance Issues

1. **Audit stats loads all logs into memory** - `get_audit_stats()` in `audit.rs` (line 281) executes `audit_log::Entity::find().all(db).await?` to compute top actions by counting in application code. This fetches every audit log row into memory instead of using a SQL `GROUP BY` aggregation. As the audit log grows, this becomes an unbounded memory allocation that will eventually cause OOM or severe latency spikes.

   ```rust
   // Current: loads ALL logs into memory to count actions
   let all_logs = audit_log::Entity::find().all(db).await?;
   let mut action_counts: HashMap<String, u64> = HashMap::new();
   for log in all_logs {
       *action_counts.entry(log.action.clone()).or_insert(0) += 1;
   }
   ```

2. **No auth-layer caching** - 4+ database queries per authenticated request for session, user, and permissions lookups. Every request hits PostgreSQL for session validation, user fetch, roles, and permissions - no in-memory caching of auth context.

3. **Hardcoded connection pool** - Pool size is hardcoded at 10 max / 1 min connections in `database.rs` and not configurable via environment variables. For a homelab with 1-5 users, 10 connections is excessive and wastes PostgreSQL memory (~10MB per connection). Conversely, if the application's concurrency needs change, a code change and rebuild is required.

4. **No automatic cache eviction** - Both `EndpointCache` and `NetworkMetricsCache` use lazy expiration (staleness checked on read only). Expired entries are never removed from the underlying `HashMap`. There is no background eviction task, no `remove()` on stale reads, and no periodic cleanup. Over time, the cache accumulates dead entries that consume memory without providing value.

5. **Unbounded cache growth** - Neither cache implementation has a maximum size limit. While `NetworkMetricsCache` is practically bounded by the number of Kubernetes namespaces, `EndpointCache` has no such natural bound. There is no LRU eviction, no max-entries cap, and no memory pressure monitoring.

6. **Audit log growth** - No automatic retention policy. `clear_old_logs()` exists but must be called manually via the admin API. Without scheduled cleanup, the audit_log table grows indefinitely, compounding the `get_audit_stats` memory issue above.

### Architectural Limitations

1. **CNPG operator overhead** - Running the CloudNativePG operator adds significant resource consumption and operational complexity for what is fundamentally a single-pod, single-user embedded database workload. The operator itself runs as a separate deployment with its own CPU/memory allocation, watches CRDs, and manages failover logic that is unnecessary when there is only one database pod. For a homelab, this is a disproportionate amount of infrastructure for the problem being solved.

2. **Cache data lost on pod restart** - All cached data (endpoint lookups, network metrics history, rate calculations) is lost when the backend pod restarts. This causes a cold-start penalty: the first requests after restart must re-populate caches via K8s API calls and metric collection. Network rate calculations need multiple samples before producing accurate sliding-window averages, meaning the dashboard shows incomplete data for several polling intervals after restart.

3. **No connection retry logic** - If PostgreSQL becomes temporarily unavailable after the initial connection is established (e.g., during a CNPG switchover, brief network partition, or pod reschedule), there is no reconnection logic. The `try_connect()` function provides graceful degradation during bootstrap but does not handle mid-operation disconnects. SeaORM's underlying connection pool (sqlx) does handle some reconnection, but the application has no explicit retry-with-backoff strategy for transient failures.

4. **hostPath coupling** - File storage is tied to the node's filesystem with no portability across nodes. If the pod is rescheduled to a different node, storage is lost. No PVC abstraction or CSI driver integration exists.

5. **SQLite already compiled** - Both `sqlx-postgres` and `sqlx-sqlite` feature flags are enabled in `Cargo.toml`, but only PostgreSQL is used at runtime. This means the binary already includes SQLite support, making a potential migration to SQLite a smaller lift than it might otherwise be.

### Operational Complexity

1. **CNPG dependency** - Requires installing and managing the CNPG operator in the cluster before Kubarr can be deployed. This adds a prerequisite step that complicates initial setup for homelab users.
2. **Backup configuration** - CNPG backups require separate configuration (S3-compatible storage, scheduled backups, retention policies). Without this, there is no automated backup - a risk for homelab users who may not configure it.
3. **Resource footprint** - CNPG operator pod + PostgreSQL pod(s) consume memory (~256MB+ for PostgreSQL, ~128MB for the operator) and CPU beyond what the application itself needs. On constrained homelab hardware (e.g., Raspberry Pi, mini PC), this overhead is significant.
4. **Port forwarding fragility** - Development workflow requires manual port-forward restart after every deployment, adding friction to the development cycle.
