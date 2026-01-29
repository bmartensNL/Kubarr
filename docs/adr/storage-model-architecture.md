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
