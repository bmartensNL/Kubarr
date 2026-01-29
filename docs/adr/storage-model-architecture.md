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

---

## Option A: Optimized PostgreSQL

**Summary:** Keep the current PostgreSQL/CloudNativePG stack but address all identified performance issues, tune CNPG configuration, and improve operational reliability. This is the lowest-risk path that delivers meaningful improvements without architectural changes.

### Quick Wins (Application-Level)

These improvements can be implemented independently with minimal risk and no schema changes.

#### 1. Environment-Configurable Connection Pool

The current hardcoded pool in `database.rs` prevents tuning without a code change and rebuild:

```rust
// Current: hardcoded values
opts.max_connections(10)
    .min_connections(1)

// Proposed: env-configurable with sensible defaults
let max_conns = std::env::var("KUBARR_DB_MAX_CONNECTIONS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(5); // Reduced default: 5 is sufficient for 1-5 users

let min_conns = std::env::var("KUBARR_DB_MIN_CONNECTIONS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(1);

opts.max_connections(max_conns)
    .min_connections(min_conns)
```

**Impact:** Reduces default PostgreSQL memory usage by ~50MB (5 connections × ~10MB each vs 10) and allows operators to tune without rebuilding. Environment variables can be set via `values.yaml` or ConfigMap.

#### 2. Audit Stats GROUP BY Query

Replace the unbounded in-memory aggregation with a SQL-level `GROUP BY`:

```rust
// Current: loads ALL audit logs into memory
let all_logs = audit_log::Entity::find().all(db).await?;
let mut action_counts: HashMap<String, u64> = HashMap::new();
for log in all_logs {
    *action_counts.entry(log.action.clone()).or_insert(0) += 1;
}

// Proposed: SQL GROUP BY - constant memory regardless of log size
use sea_orm::{FromQueryResult, QuerySelect};

#[derive(Debug, FromQueryResult)]
struct ActionCount {
    action: String,
    count: i64,
}

let action_counts = audit_log::Entity::find()
    .select_only()
    .column(audit_log::Column::Action)
    .column_as(audit_log::Column::Id.count(), "count")
    .group_by(audit_log::Column::Action)
    .into_model::<ActionCount>()
    .all(db)
    .await?;
```

**Impact:** Eliminates the most critical memory issue. With 100K audit logs, the current approach allocates ~50MB+ for deserialized models; the GROUP BY approach returns only the aggregated counts (a few KB regardless of table size). This prevents OOM risks as the audit log grows.

#### 3. Background Cache Cleanup

Add a `tokio::spawn` interval task to evict expired entries from both caches:

```rust
// In application startup, after AppState is created:
let cache_clone = app_state.endpoint_cache.clone();
let metrics_clone = app_state.network_metrics_cache.clone();

tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
    loop {
        interval.tick().await;
        cache_clone.evict_expired().await;
        metrics_clone.evict_stale().await;
        tracing::debug!("Cache cleanup completed");
    }
});
```

This requires adding `evict_expired()` / `evict_stale()` methods to `EndpointCache` and `NetworkMetricsCache` that iterate the HashMap and remove entries older than the TTL/max_age.

**Impact:** Prevents unbounded memory growth from dead cache entries. For `EndpointCache`, this bounds memory to active endpoints only. For `NetworkMetricsCache`, stale namespace entries from deleted namespaces are cleaned up.

#### 4. Scheduled Audit Log Retention

Automate the existing `clear_old_logs()` function via a background task instead of requiring manual admin API calls:

```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(86400)); // Daily
    loop {
        interval.tick().await;
        let retention_days = std::env::var("KUBARR_AUDIT_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(90); // Default: 90 days
        if let Some(db) = db_conn.read().await.as_ref() {
            match clear_old_logs(db, retention_days).await {
                Ok(count) => tracing::info!("Audit cleanup: removed {} old entries", count),
                Err(e) => tracing::warn!("Audit cleanup failed: {}", e),
            }
        }
    }
});
```

**Impact:** Prevents indefinite table growth. With 90-day retention and typical homelab activity (~100-500 events/day), the audit_log table stays under 50K rows (~5MB), keeping the GROUP BY query fast and PostgreSQL VACUUM efficient.

### CNPG Tuning (Infrastructure-Level)

These improvements optimize the CloudNativePG cluster configuration for homelab workloads.

#### 1. Separate WAL Volume

PostgreSQL Write-Ahead Logging (WAL) benefits significantly from dedicated I/O:

```yaml
# CNPG Cluster spec
spec:
  storage:
    size: 5Gi
    storageClass: local-path
  walStorage:
    size: 2Gi
    storageClass: local-path
```

**Impact:** Separating WAL from data storage can improve write IOPS by up to 2x on spinning disks and reduce I/O contention on SSDs. For homelab NVMe/SSD setups, the benefit is moderate but still measurable (~20-30% write throughput improvement) due to reduced fsync contention.

#### 2. Memory and CPU Allocation

Allocate ~75% of available resources to PostgreSQL for optimal buffer pool sizing:

```yaml
spec:
  resources:
    requests:
      memory: 256Mi
      cpu: 100m
    limits:
      memory: 512Mi
      cpu: 500m
  postgresql:
    parameters:
      shared_buffers: "128MB"        # ~25% of memory limit
      effective_cache_size: "384MB"  # ~75% of memory limit
      work_mem: "4MB"               # Per-operation sort memory
      maintenance_work_mem: "64MB"  # For VACUUM, CREATE INDEX
      wal_buffers: "4MB"
      max_connections: "20"         # Match app pool + headroom
```

**Impact:** Proper `shared_buffers` and `effective_cache_size` settings allow PostgreSQL to cache hot data (user sessions, roles, permissions) in memory, reducing disk reads for the auth-layer's 4+ queries per request. For Kubarr's 22-table schema with typical homelab data sizes (<100MB total), most of the working set fits in shared buffers.

#### 3. Dynamic PVC Provisioning

Use StorageClass with `allowVolumeExpansion: true` to enable online volume resizing:

```yaml
spec:
  storage:
    size: 5Gi
    storageClass: local-path  # Or longhorn, openebs-lvm for expansion support
    pvcTemplate:
      accessModes:
        - ReadWriteOnce
```

**Impact:** Eliminates the need to recreate PVCs when storage grows. Operators can resize with `kubectl patch` rather than performing backup/restore cycles. Note: `local-path` provisioner does not support expansion; production homelab setups should consider Longhorn or OpenEBS for this capability.

#### 4. Volume Snapshots for Backups

Leverage Kubernetes VolumeSnapshot API for consistent point-in-time backups:

```yaml
apiVersion: snapshot.storage.k8s.io/v1
kind: VolumeSnapshot
metadata:
  name: kubarr-db-snapshot
spec:
  volumeSnapshotClassName: csi-snapshotter
  source:
    persistentVolumeClaimName: kubarr-db-pvc
```

Alternatively, CNPG's built-in backup to S3-compatible storage (MinIO, Backblaze B2):

```yaml
spec:
  backup:
    barmanObjectStore:
      destinationPath: "s3://kubarr-backups/"
      endpointURL: "https://s3.us-west-000.backblazeb2.com"
      s3Credentials:
        accessKeyId:
          name: backup-creds
          key: ACCESS_KEY_ID
        secretAccessKey:
          name: backup-creds
          key: SECRET_ACCESS_KEY
    retentionPolicy: "30d"
```

**Impact:** Provides automated, consistent backups without manual intervention. CNPG's Barman integration handles WAL archiving and point-in-time recovery (PITR). Backblaze B2 free tier (10GB) is sufficient for typical homelab database sizes.

### High Availability Considerations

CNPG supports primary + replica configurations with automatic failover:

```yaml
spec:
  instances: 2  # Primary + 1 streaming replica
  enableSuperuserAccess: false
  primaryUpdateStrategy: unsupervised
```

**Assessment for homelab:** HA is generally **overkill** for a single-user homelab deployment. The added resource cost (2x PostgreSQL memory/CPU, WAL streaming overhead) outweighs the benefit when:
- Acceptable downtime is minutes, not seconds
- There is one operator who can manually intervene
- Pod restarts typically complete in 10-30 seconds

**When HA makes sense:**
- Multi-user homelab serving a household (3-5 concurrent users)
- Running on a multi-node cluster where node failure is a realistic scenario
- Kubarr manages critical infrastructure where downtime causes cascading issues

### Pros and Cons

| Aspect | Assessment |
|--------|------------|
| **Pros** | |
| Minimal migration effort | No schema changes, no data migration, no deployment model changes |
| Low risk | Each quick win can be deployed independently and rolled back |
| Preserves CNPG features | Automated failover, backup, WAL archiving remain available |
| Full SQL capabilities | All PostgreSQL features (LISTEN/NOTIFY, advanced indexing, JSON operators, full-text search) remain available |
| SeaORM compatibility | No ORM changes needed; existing migrations continue to work |
| Incremental improvement | Quick wins can be shipped immediately while CNPG tuning follows |
| **Cons** | |
| CNPG operator overhead remains | Operator pod (~128MB RAM) continues running for a single database instance |
| Operational complexity persists | Homelab users still need to install and manage the CNPG operator |
| Resource floor unchanged | PostgreSQL pod + operator pod baseline remains ~384MB+ RAM |
| Does not simplify deployment | Still requires CNPG CRDs, operator deployment, and cluster resource creation |
| Limited portability | CNPG is Kubernetes-specific; no path to running Kubarr outside K8s |

### Migration Effort

**Effort: Minimal (1-2 days of development)**

| Change | Effort | Risk |
|--------|--------|------|
| Configurable connection pool | ~1 hour | Very low - additive change, defaults preserved |
| Audit stats GROUP BY | ~2 hours | Low - query change with same output format |
| Background cache cleanup | ~2 hours | Low - additive background task |
| Scheduled audit retention | ~1 hour | Low - wraps existing `clear_old_logs()` |
| CNPG tuning (values.yaml) | ~2 hours | Low - configuration changes, rollback via Helm |
| HA setup (if desired) | ~4 hours | Medium - requires testing failover scenarios |

No data migration is required. No schema changes are needed. All changes are backward-compatible.

### Resource Impact

| Resource | Current | After Quick Wins | After CNPG Tuning |
|----------|---------|------------------|-------------------|
| PostgreSQL memory | ~256MB (default) | ~256MB (unchanged) | ~512MB (tuned buffers) |
| CNPG operator | ~128MB | ~128MB (unchanged) | ~128MB (unchanged) |
| DB connections | 10 max (hardcoded) | 5 max (configurable) | 5 max (configurable) |
| Connection memory | ~100MB (10 × 10MB) | ~50MB (5 × 10MB) | ~50MB (5 × 10MB) |
| Audit log table | Unbounded growth | Bounded by retention policy | Bounded by retention policy |
| Cache memory | Unbounded (lazy eviction) | Bounded (active eviction) | Bounded (active eviction) |
| **Total DB footprint** | **~484MB** | **~434MB** | **~690MB (with tuning)** |

Note: CNPG tuning increases PostgreSQL memory allocation intentionally to improve cache hit rates, which reduces disk I/O and improves query latency. The trade-off is worthwhile if the host has sufficient RAM (4GB+).

### Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| GROUP BY query returns different format | Low | Low | Unit test comparing old vs new output |
| Background task panics | Very low | Low | `tokio::spawn` isolates panics; add error logging |
| CNPG tuning causes instability | Low | Medium | Test in dev cluster first; Helm rollback available |
| Connection pool too small | Low | Low | Configurable via env var; adjust without rebuild |
| Audit retention deletes needed logs | Low | Medium | Default 90 days is conservative; env-configurable |

**Overall risk: Low.** All changes are incremental, independently deployable, and reversible. The quick wins address real issues with proven patterns. CNPG tuning follows well-documented PostgreSQL best practices.

---

## Option B: SQLite + Litestream

**Summary:** Replace PostgreSQL with an embedded SQLite database running inside the backend process, eliminating the CNPG operator and external database pod entirely. Continuous replication to S3-compatible storage (Backblaze B2) is handled by Litestream running as a sidecar container. The Kubernetes deployment model changes from a Deployment to a StatefulSet with `replicas: 1`.

### SeaORM Feature Flag Change

The Rust backend already compiles both PostgreSQL and SQLite support via SeaORM feature flags in `Cargo.toml`:

```toml
# Current Cargo.toml - both backends already compiled
sea-orm = { version = "1.1", features = ["sqlx-postgres", "sqlx-sqlite", "runtime-tokio-native-tls", "macros", "with-chrono"] }
sea-orm-migration = { version = "1.1", features = ["sqlx-postgres", "sqlx-sqlite", "runtime-tokio-native-tls"] }
```

For a SQLite-only deployment, the feature flags would change to remove the PostgreSQL dependency:

```toml
# Option B: SQLite-only (reduces binary size and compile time)
sea-orm = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-native-tls", "macros", "with-chrono"] }
sea-orm-migration = { version = "1.1", features = ["sqlx-sqlite", "runtime-tokio-native-tls"] }
```

Alternatively, both can be kept (as today) and the database backend selected at runtime via the `DATABASE_URL` scheme:

```rust
// SQLite: DATABASE_URL=sqlite:///data/kubarr.db?mode=rwc
// PostgreSQL: DATABASE_URL=postgres://user:pass@host:5432/kubarr
```

SeaORM's `Database::connect()` automatically selects the correct driver based on the URL scheme. This means the same binary can support both backends, controlled entirely by configuration.

**Recommendation:** Keep both feature flags compiled (as today) to preserve the ability to use either backend. The binary size increase is negligible (~2MB), and it provides deployment flexibility.

### Data Type Compatibility

All 22 entity models use SeaORM-portable types that work identically across PostgreSQL and SQLite:

| SeaORM Type | PostgreSQL Mapping | SQLite Mapping | Compatible? |
|-------------|-------------------|----------------|-------------|
| `i64` (primary keys) | `BIGINT` | `INTEGER` (64-bit) | ✅ Yes |
| `String` | `VARCHAR`/`TEXT` | `TEXT` | ✅ Yes |
| `DateTimeUtc` (chrono) | `TIMESTAMPTZ` | `TEXT` (ISO 8601) | ✅ Yes |
| `bool` | `BOOLEAN` | `INTEGER` (0/1) | ✅ Yes |
| `Option<T>` | `NULLABLE` | `NULLABLE` | ✅ Yes |
| `Json` (serde_json) | `JSONB` | `TEXT` (JSON string) | ✅ Yes¹ |

¹ SQLite stores JSON as text. SeaORM handles serialization/deserialization transparently. However, PostgreSQL JSON operators (`->`, `->>`, `@>`, `#>`) are not available in SQLite. The current codebase uses `serde_json::Value` for JSON columns and deserializes in application code, so this is not an issue.

**Verified:** All 22 entity models in `code/backend/src/models/` use `i64` primary keys and `DateTimeUtc` timestamps. No PostgreSQL-specific column types (arrays, enums, custom types) are used. The SeaORM abstraction layer handles type mapping transparently.

### Connection Configuration Change

The `database.rs` connection setup changes minimally for SQLite:

```rust
// Current PostgreSQL connection
let mut opts = ConnectOptions::new("postgres://user:pass@host:5432/kubarr");
opts.max_connections(10)
    .min_connections(1)
    .connect_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(600))
    .sqlx_logging(false);

// SQLite connection (Option B)
let mut opts = ConnectOptions::new("sqlite:///data/kubarr.db?mode=rwc");
opts.max_connections(1)       // SQLite: single-writer, one connection for writes
    .min_connections(1)       // Keep connection alive
    .connect_timeout(Duration::from_secs(10))
    .idle_timeout(Duration::from_secs(600))
    .sqlx_logging(false);

// Critical: enable WAL mode for concurrent reads during writes
db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
db.execute_unprepared("PRAGMA busy_timeout=5000").await?;
db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
db.execute_unprepared("PRAGMA foreign_keys=ON").await?;
```

**Key SQLite PRAGMAs:**
- `journal_mode=WAL` — Enables Write-Ahead Logging for concurrent read access during writes. Essential for Litestream replication.
- `busy_timeout=5000` — Wait up to 5 seconds for write lock instead of immediately returning SQLITE_BUSY.
- `synchronous=NORMAL` — Balanced durability/performance. WAL mode makes this safe; Litestream provides the durability guarantee via S3 replication.
- `foreign_keys=ON` — SQLite disables foreign key enforcement by default. Must be enabled per connection.

### StatefulSet Deployment

SQLite requires a persistent volume mounted into the pod. The Kubernetes deployment model changes from `Deployment` to `StatefulSet` to ensure stable storage identity:

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: {{ include "kubarr.fullname" . }}-backend
  namespace: {{ .Values.namespace.name }}
  labels:
    {{- include "kubarr.labels" . | nindent 4 }}
    app.kubernetes.io/component: backend
spec:
  serviceName: {{ include "kubarr.fullname" . }}-backend
  replicas: 1  # CRITICAL: SQLite requires single-writer; never scale beyond 1
  updateStrategy:
    type: Recreate  # Must stop old pod before starting new one (no rolling updates)
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ include "kubarr.name" . }}-backend
      app.kubernetes.io/instance: {{ .Release.Name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ include "kubarr.name" . }}-backend
        app.kubernetes.io/instance: {{ .Release.Name }}
    spec:
      serviceAccountName: {{ include "kubarr.serviceAccountName" . }}
      # Init container: restore SQLite database from S3 via Litestream before backend starts
      initContainers:
        - name: litestream-restore
          image: litestream/litestream:0.3
          args: ["restore", "-if-db-not-exists", "/data/kubarr.db"]
          volumeMounts:
            - name: data
              mountPath: /data
          envFrom:
            - secretRef:
                name: {{ include "kubarr.fullname" . }}-litestream
      containers:
        # Backend API container
        - name: backend
          image: "{{ .Values.backend.image.repository }}:{{ .Values.backend.image.tag }}"
          ports:
            - name: http
              containerPort: {{ .Values.backend.service.targetPort }}
              protocol: TCP
          env:
            - name: DATABASE_URL
              value: "sqlite:///data/kubarr.db?mode=rwc"
            {{- toYaml .Values.backend.env | nindent 12 }}
          volumeMounts:
            - name: data
              mountPath: /data
          livenessProbe:
            {{- toYaml .Values.backend.livenessProbe | nindent 12 }}
          readinessProbe:
            {{- toYaml .Values.backend.readinessProbe | nindent 12 }}
          resources:
            {{- toYaml .Values.backend.resources | nindent 12 }}
        # Litestream sidecar: continuous replication to S3
        - name: litestream
          image: litestream/litestream:0.3
          args: ["replicate"]
          volumeMounts:
            - name: data
              mountPath: /data
            - name: litestream-config
              mountPath: /etc/litestream.yml
              subPath: litestream.yml
          envFrom:
            - secretRef:
                name: {{ include "kubarr.fullname" . }}-litestream
          resources:
            requests:
              memory: 32Mi
              cpu: 10m
            limits:
              memory: 64Mi
              cpu: 50m
      volumes:
        - name: litestream-config
          configMap:
            name: {{ include "kubarr.fullname" . }}-litestream
  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: ["ReadWriteOnce"]
        storageClassName: {{ .Values.storage.storageClass | default "local-path" }}
        resources:
          requests:
            storage: {{ .Values.storage.size | default "1Gi" }}
```

**Key design decisions:**
- `replicas: 1` — SQLite's single-writer constraint means only one pod can write at a time. Attempting to scale beyond 1 would cause `SQLITE_BUSY` errors.
- `updateStrategy: Recreate` — The old pod must fully stop (releasing the SQLite file lock) before the new pod starts. Rolling updates would cause two pods to contend for the same database file.
- `volumeClaimTemplates` — StatefulSet provides stable PVC identity (`data-kubarr-backend-0`) that survives pod restarts and rescheduling.
- Init container runs `litestream restore` with `-if-db-not-exists` — Only restores from S3 on first deploy or after data loss. Subsequent restarts use the existing PVC data.

### Litestream Sidecar Pattern

Litestream continuously replicates SQLite's WAL (Write-Ahead Log) to S3-compatible object storage. This provides near-real-time backup without any application code changes.

**How it works:**

1. **Init container (restore):** Before the backend starts, Litestream downloads the latest database snapshot and WAL segments from S3, reconstructing the full SQLite database. The `-if-db-not-exists` flag skips restore if the database already exists on the PVC.

2. **Sidecar container (replicate):** Runs alongside the backend, watching the SQLite WAL file for changes. New WAL frames are uploaded to S3 within seconds of being written. Litestream performs periodic snapshots (default: every 24 hours) for faster future restores.

**Litestream configuration:**

```yaml
# ConfigMap: litestream.yml
dbs:
  - path: /data/kubarr.db
    replicas:
      - type: s3
        bucket: kubarr-backups
        path: db
        endpoint: https://s3.us-west-000.backblazeb2.com
        region: us-west-000
        retention: 168h          # Keep 7 days of WAL segments
        retention-check-interval: 1h
        snapshot-interval: 24h   # Full snapshot daily
        sync-interval: 1s       # Replicate WAL changes every second
        validation-interval: 12h # Verify replica integrity every 12 hours
```

**Recovery scenarios:**

| Scenario | Recovery Method | Downtime | Data Loss |
|----------|----------------|----------|-----------|
| Pod restart (same node) | PVC data intact; no restore needed | ~5-10 seconds (pod startup) | None |
| Pod reschedule (different node) | PVC may need re-provisioning; Litestream restores from S3 | ~30-60 seconds (restore + startup) | ≤1 second (last unreplicated WAL frame) |
| PVC data corruption | Litestream restores latest snapshot + WAL from S3 | ~30-60 seconds | ≤1 second |
| S3 bucket loss | PVC still has local data; manual backup needed | None (until PVC loss) | None (if PVC intact) |
| Complete disaster (PVC + S3) | Full data loss | N/A | All data |

### S3-Compatible Backup (Backblaze B2)

Backblaze B2 is the recommended S3-compatible storage for homelab use:

- **Free tier:** 10 GB storage, 1 GB/day download — more than sufficient for SQLite databases (typical Kubarr DB: <50MB)
- **S3-compatible API:** Litestream natively supports B2's S3 endpoint
- **Pricing beyond free tier:** $0.006/GB/month storage, $0.01/GB egress — negligible for database backups
- **Regions:** US West, US East, EU Central — low latency for most homelab locations

**Setup:**

```yaml
# Kubernetes Secret for Litestream S3 credentials
apiVersion: v1
kind: Secret
metadata:
  name: kubarr-litestream
type: Opaque
stringData:
  LITESTREAM_ACCESS_KEY_ID: "your-b2-application-key-id"
  LITESTREAM_SECRET_ACCESS_KEY: "your-b2-application-key"
```

**Alternative S3 backends:** MinIO (self-hosted), Wasabi ($6.99/TB/month), AWS S3, or any S3-compatible storage. Litestream is backend-agnostic.

**Offline/air-gapped fallback:** For environments without internet access, Litestream can replicate to a local MinIO instance or an NFS-mounted directory using the `file` replica type:

```yaml
replicas:
  - type: file
    path: /backup/kubarr-db
    retention: 168h
```

### Critical Constraints

#### 1. Single-Pod Only (Single-Writer Limitation)

SQLite supports only one concurrent writer. All write operations are serialized through a single write lock. This means:

- **No horizontal scaling** — Only one backend pod can exist. Attempting `replicas: 2` would cause `SQLITE_BUSY` errors on writes.
- **No read replicas** — Unlike PostgreSQL, there is no streaming replication for read scaling.
- **Write throughput** — SQLite in WAL mode handles ~50,000-100,000 simple writes/second on SSD. For Kubarr's homelab workload (1-5 users, <100 writes/minute), this is orders of magnitude more than sufficient.
- **Concurrent reads** — WAL mode allows unlimited concurrent readers even during writes. Read throughput is not a concern.

**Assessment for Kubarr:** The single-writer limitation is a non-issue for the homelab use case. Kubarr is already designed for single-pod deployment (`replicas: 1` in the current Deployment). The write workload (session updates, audit logs, configuration changes) is extremely light.

#### 2. Brief Downtime on Pod Reschedule

The `Recreate` update strategy means the old pod must fully terminate before the new pod starts. During this window:

- **Duration:** Typically 10-30 seconds (pod termination + new pod scheduling + Litestream restore if PVC is lost + app startup)
- **Impact:** API requests return 503 during the transition. WebSocket connections are dropped and must reconnect.
- **Mitigation:** Kubernetes `terminationGracePeriodSeconds` allows the backend to complete in-flight requests. Litestream's init container restore is fast for small databases (<1 second for <50MB).

**Assessment:** This downtime window is identical to the current behavior with PostgreSQL when the backend pod restarts. The CNPG database pod also has its own restart/failover time. In practice, SQLite may have *less* total downtime because there is no external database pod to restart or failover.

#### 3. No LISTEN/NOTIFY

PostgreSQL's `LISTEN/NOTIFY` mechanism provides real-time database-level event notifications. SQLite has no equivalent feature. This affects:

- **Current usage:** The Kubarr codebase does not currently use `LISTEN/NOTIFY`. WebSocket notifications are handled via Tokio broadcast channels in `AppState` (`NetworkMetricsBroadcast`, `BootstrapBroadcast`), not database triggers.
- **Future impact:** If real-time database change notifications were ever needed, they would need to be implemented via application-level event emission (e.g., publish to a Tokio channel after each write) rather than database triggers.

**Assessment:** No impact on current functionality. The application already uses in-memory broadcast channels for real-time events.

#### 4. No Advanced PostgreSQL Features

Migrating to SQLite means losing access to:

- **Full-text search (tsvector)** — Not currently used. If needed, SQLite's FTS5 extension provides similar functionality.
- **Array columns** — Not currently used in any of the 22 entity models.
- **JSONB operators** — Not used in queries; JSON is deserialized in application code via `serde_json`.
- **Advanced indexing (GIN, GiST, BRIN)** — Not currently used. SQLite supports standard B-tree indexes, which are sufficient for the current query patterns.
- **Window functions** — SQLite supports window functions (since 3.25.0), so this is not a limitation.
- **CTEs** — SQLite supports CTEs (since 3.8.3), so this is not a limitation.

**Assessment:** No current Kubarr functionality depends on PostgreSQL-specific features. All queries use standard SQL that is portable across both backends.

### Data Migration Approach

Migrating existing data from PostgreSQL to SQLite requires a one-time export/import process:

#### Step 1: Export from PostgreSQL

```bash
# Export each table as SQL INSERT statements (portable format)
pg_dump --data-only --inserts --no-owner --no-privileges \
  --exclude-table=sea_orm_migration \
  -d kubarr > kubarr_data.sql
```

Using `--inserts` instead of `COPY` ensures the SQL is SQLite-compatible. The `sea_orm_migration` table is excluded because SeaORM recreates it automatically.

#### Step 2: Create SQLite Database

```bash
# Run the backend with SQLite URL to trigger SeaORM migrations
DATABASE_URL=sqlite:///tmp/kubarr.db?mode=rwc cargo run
# Or use the migration CLI:
sea-orm-cli migrate up -d /tmp/kubarr.db
```

SeaORM's migration framework handles the SQLite schema creation. The same migration files work for both PostgreSQL and SQLite because they use SeaORM's database-agnostic DDL API.

#### Step 3: Import Data

```bash
# Clean up PostgreSQL-specific syntax from the dump
sed -e "s/true/1/g" -e "s/false/0/g" \
    -e "s/::.*//g" \
    -e "/^SET /d" -e "/^SELECT pg_/d" \
    kubarr_data.sql > kubarr_data_sqlite.sql

# Import into SQLite
sqlite3 /tmp/kubarr.db < kubarr_data_sqlite.sql
```

**Data type conversions during migration:**
- `boolean` → `0`/`1` (SQLite stores booleans as integers)
- `timestamptz` → ISO 8601 text (SeaORM handles this transparently via chrono)
- `bigint` → `INTEGER` (SQLite's INTEGER is 64-bit, matching PostgreSQL's BIGINT)
- PostgreSQL type casts (`::text`, `::integer`) → removed (SQLite doesn't use cast syntax in this form)

#### Step 4: Verify

```bash
# Count rows in each table to verify migration completeness
sqlite3 /tmp/kubarr.db "SELECT name, (SELECT COUNT(*) FROM [name]) FROM sqlite_master WHERE type='table';"
```

**Alternative approach:** A Rust migration tool could be written to read from PostgreSQL and write to SQLite using SeaORM, providing type-safe migration with automatic conversion. This is more robust but requires development effort.

#### Migration Effort Estimate

| Task | Effort | Risk |
|------|--------|------|
| SQLite PRAGMAs in `database.rs` | ~2 hours | Low - well-documented SQLite best practices |
| Migration compatibility testing | ~4 hours | Medium - verify all 23 migrations run on SQLite |
| StatefulSet Helm template | ~4 hours | Low - template conversion with Litestream sidecar |
| Litestream ConfigMap and Secret | ~2 hours | Low - standard Kubernetes resources |
| Data export/import tooling | ~4 hours | Medium - type conversion edge cases |
| Integration testing | ~8 hours | Medium - verify all 22 entities CRUD on SQLite |
| Remove CNPG dependency | ~2 hours | Low - delete CRDs, operator, and RBAC rules |
| Documentation | ~2 hours | Low - update README and deployment guide |
| **Total** | **~28 hours (3-4 days)** | **Medium overall** |

### Pros and Cons

| Aspect | Assessment |
|--------|------------|
| **Pros** | |
| Eliminates CNPG operator | No operator pod (~128MB RAM), no CRDs, no operator upgrades to manage |
| Eliminates PostgreSQL pod | No separate database pod (~256MB+ RAM), no connection pool overhead |
| Dramatically simpler deployment | One StatefulSet with a sidecar replaces Deployment + CNPG Cluster + operator |
| Lower resource footprint | SQLite is embedded in the backend process; total savings ~384MB+ RAM |
| Automated backup via Litestream | Continuous S3 replication with <1s RPO; simpler than CNPG Barman config |
| No network database latency | Queries execute in-process via file I/O, not over TCP. ~10x faster for simple queries |
| SQLite already compiled | Both `sqlx-postgres` and `sqlx-sqlite` feature flags are in `Cargo.toml`; no new dependencies |
| Ideal for homelab scale | SQLite handles millions of rows; Kubarr's 22 tables with <100K rows total is trivial |
| Fewer failure modes | No database connection failures, no network partitions, no connection pool exhaustion |
| **Cons** | |
| Single-writer limitation | Cannot scale beyond 1 replica; writes are serialized (but sufficient for homelab) |
| Recreate update strategy | Brief downtime (~10-30s) during pod updates; no zero-downtime rolling updates |
| No LISTEN/NOTIFY | Database-level event notifications unavailable (not currently used) |
| Migration effort required | ~3-4 days of development to convert, test, and validate data migration |
| S3 dependency for backup | Litestream requires S3-compatible storage; adds external dependency (but free via B2) |
| Less ecosystem tooling | No pgAdmin, no `psql`, limited debugging tools vs PostgreSQL's rich ecosystem |
| StatefulSet complexity | PVC management, volume provisioning, and storage class requirements |
| No advanced SQL features | No PostgreSQL-specific features (though none are currently used) |

### Resource Impact

| Resource | Current (PostgreSQL + CNPG) | Option B (SQLite + Litestream) | Savings |
|----------|---------------------------|-------------------------------|---------|
| CNPG operator pod | ~128MB RAM, ~50m CPU | Eliminated | 128MB RAM |
| Database pod | ~256MB RAM, ~100m CPU | Eliminated | 256MB RAM |
| Litestream sidecar | N/A | ~32-64MB RAM, ~10m CPU | -64MB RAM |
| Backend memory (DB) | ~10MB (connection pool) | ~5MB (SQLite in-process) | 5MB RAM |
| Database connections | 10 TCP connections | 1 file handle | Negligible |
| Storage volumes | CNPG PVC (5Gi) + WAL PVC (2Gi) | Single PVC (1Gi) | 6Gi disk |
| S3 backup storage | Optional (CNPG Barman) | Required (Litestream) | ~Same cost |
| **Total RAM savings** | | | **~325MB** |
| **Total pod count** | 3 (backend + PG + operator) | 1 (backend + sidecar) | 2 fewer pods |

### Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Migration data loss | Low | High | Test migration on copy; verify row counts; keep PostgreSQL running during transition |
| SQLite corruption | Very low | High | Litestream provides continuous backup; WAL mode is mature and well-tested |
| SeaORM migration incompatibility | Medium | Medium | Run all 23 migrations on SQLite in CI; fix any PostgreSQL-specific DDL |
| Litestream S3 outage | Low | Low | Local PVC has full data; S3 is only for backup, not runtime |
| Write contention under load | Very low | Low | SQLite handles >50K writes/sec; Kubarr does <100 writes/min |
| Future need for PostgreSQL features | Low | Medium | Both feature flags remain compiled; can switch back via `DATABASE_URL` |
| PVC data loss on node failure | Low | Medium | Litestream restores from S3 within seconds; RPO <1 second |
| StatefulSet operational complexity | Low | Low | Standard K8s pattern; well-documented; simpler than CNPG operator |

**Overall risk: Medium.** The migration itself carries moderate risk (data conversion, migration compatibility), but the runtime risk is lower than the current architecture (fewer moving parts, fewer failure modes). The ability to keep both feature flags and switch via `DATABASE_URL` provides a safe rollback path.
