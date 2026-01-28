use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};

use sea_orm::DatabaseConnection;

use crate::services::audit::AuditService;
use crate::services::cadvisor::NamespaceNetworkMetrics;
use crate::services::catalog::AppCatalog;
use crate::services::k8s::K8sClient;
use crate::services::notification::NotificationService;
use crate::services::proxy::ProxyService;

/// Database connection type alias
pub type DbConn = DatabaseConnection;

/// Cached app endpoint with expiration
#[derive(Clone)]
pub struct CachedEndpoint {
    pub base_url: String,
    pub base_path: Option<String>,
    pub expires_at: Instant,
}

/// Cache for app service endpoints (avoids K8s API calls on every request)
#[derive(Clone, Default)]
pub struct EndpointCache {
    cache: Arc<RwLock<HashMap<String, CachedEndpoint>>>,
    ttl: Duration,
}

/// Number of samples to keep for sliding window average
const RATE_WINDOW_SIZE: usize = 5;

/// A single rate sample
#[derive(Clone, Default)]
pub struct RateSample {
    pub rx_bytes: f64,
    pub tx_bytes: f64,
    pub rx_packets: f64,
    pub tx_packets: f64,
    pub rx_errors: f64,
    pub tx_errors: f64,
    pub rx_dropped: f64,
    pub tx_dropped: f64,
}

/// Cached rates for a namespace (sliding window average)
#[derive(Clone, Default)]
pub struct CachedRates {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_packets_per_sec: f64,
    pub tx_packets_per_sec: f64,
    pub rx_errors_per_sec: f64,
    pub tx_errors_per_sec: f64,
    pub rx_dropped_per_sec: f64,
    pub tx_dropped_per_sec: f64,
}

/// Sliding window of rate samples for smooth averaging
#[derive(Clone, Default)]
pub struct RateHistory {
    samples: Vec<RateSample>,
}

impl RateHistory {
    /// Add a new sample and return the sliding window average
    pub fn add_sample(&mut self, sample: RateSample) -> CachedRates {
        self.samples.push(sample);

        // Keep only the last N samples
        if self.samples.len() > RATE_WINDOW_SIZE {
            self.samples.remove(0);
        }

        self.average()
    }

    /// Compute average of all samples in the window
    pub fn average(&self) -> CachedRates {
        if self.samples.is_empty() {
            return CachedRates::default();
        }

        let count = self.samples.len() as f64;
        let mut avg = CachedRates::default();

        for s in &self.samples {
            avg.rx_bytes_per_sec += s.rx_bytes;
            avg.tx_bytes_per_sec += s.tx_bytes;
            avg.rx_packets_per_sec += s.rx_packets;
            avg.tx_packets_per_sec += s.tx_packets;
            avg.rx_errors_per_sec += s.rx_errors;
            avg.tx_errors_per_sec += s.tx_errors;
            avg.rx_dropped_per_sec += s.rx_dropped;
            avg.tx_dropped_per_sec += s.tx_dropped;
        }

        avg.rx_bytes_per_sec /= count;
        avg.tx_bytes_per_sec /= count;
        avg.rx_packets_per_sec /= count;
        avg.tx_packets_per_sec /= count;
        avg.rx_errors_per_sec /= count;
        avg.tx_errors_per_sec /= count;
        avg.rx_dropped_per_sec /= count;
        avg.tx_dropped_per_sec /= count;

        avg
    }
}

/// Cached network metrics entry with timestamp for rate calculation
#[derive(Clone)]
pub struct CachedNetworkMetrics {
    pub metrics: NamespaceNetworkMetrics,
    pub timestamp: Instant,
    /// Sliding window of rate samples for averaging
    pub rate_history: RateHistory,
    /// Last calculated averaged rates
    pub last_rates: CachedRates,
}

/// Cache for network metrics to calculate rates from cumulative counters
#[derive(Clone, Default)]
pub struct NetworkMetricsCache {
    cache: Arc<RwLock<HashMap<String, CachedNetworkMetrics>>>,
    /// Maximum age before cache entry is considered stale (5 minutes)
    max_age: Duration,
}

impl NetworkMetricsCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Get cached metrics for a namespace
    pub async fn get(&self, namespace: &str) -> Option<CachedNetworkMetrics> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(namespace) {
            // Check if not stale
            if entry.timestamp.elapsed() < self.max_age {
                return Some(entry.clone());
            }
        }
        None
    }

    /// Update cache with new metrics and a new rate sample
    /// Returns the sliding window average of rates
    pub async fn add_sample(
        &self,
        namespace: &str,
        metrics: NamespaceNetworkMetrics,
        sample: RateSample,
    ) -> CachedRates {
        let mut cache = self.cache.write().await;

        let entry = cache
            .entry(namespace.to_string())
            .or_insert_with(|| CachedNetworkMetrics {
                metrics: metrics.clone(),
                timestamp: Instant::now(),
                rate_history: RateHistory::default(),
                last_rates: CachedRates::default(),
            });

        // Update metrics and timestamp
        entry.metrics = metrics;
        entry.timestamp = Instant::now();

        // Add sample to history and get averaged rates
        let averaged_rates = entry.rate_history.add_sample(sample);
        entry.last_rates = averaged_rates.clone();

        averaged_rates
    }

    /// Calculate rate between two values over elapsed time
    /// Returns None if rate cannot be calculated (counter reset, etc.)
    pub fn rate_from_delta(current: u64, previous: u64, elapsed_secs: f64) -> Option<f64> {
        if elapsed_secs > 0.1 && current >= previous {
            Some((current - previous) as f64 / elapsed_secs)
        } else {
            // Counter reset, invalid, or too short interval
            None
        }
    }

    /// Apply exponential moving average smoothing to a rate
    /// This prevents abrupt jumps from actual values to 0 when traffic is bursty
    /// alpha controls responsiveness: higher = more responsive, lower = smoother
    pub fn smooth_rate(new_rate: f64, old_rate: f64, alpha: f64) -> f64 {
        if old_rate == 0.0 {
            // No previous value, use new rate directly
            new_rate
        } else if new_rate == 0.0 {
            // New measurement is 0, decay slowly toward 0
            old_rate * (1.0 - alpha)
        } else {
            // Normal EMA calculation
            alpha * new_rate + (1.0 - alpha) * old_rate
        }
    }
}

impl EndpointCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Get cached endpoint for an app (base_url, base_path)
    pub async fn get(&self, app_name: &str) -> Option<(String, Option<String>)> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(app_name) {
            if entry.expires_at > Instant::now() {
                return Some((entry.base_url.clone(), entry.base_path.clone()));
            }
        }
        None
    }

    /// Cache an endpoint for an app
    pub async fn set(&self, app_name: &str, base_url: String, base_path: Option<String>) {
        let mut cache = self.cache.write().await;
        cache.insert(
            app_name.to_string(),
            CachedEndpoint {
                base_url,
                base_path,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// Invalidate cache for an app (e.g., when app is restarted)
    pub async fn invalidate(&self, app_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(app_name);
    }
}

/// Shared K8s client state
pub type SharedK8sClient = Arc<RwLock<Option<K8sClient>>>;

/// Shared app catalog state
pub type SharedCatalog = Arc<RwLock<AppCatalog>>;

/// Broadcast channel for real-time network metrics to WebSocket clients
pub type NetworkMetricsBroadcast = broadcast::Sender<String>;

/// Broadcast channel for bootstrap progress to WebSocket clients
pub type BootstrapBroadcast = broadcast::Sender<String>;

/// Shared database connection (optional until PostgreSQL is installed)
pub type SharedDbConn = Arc<RwLock<Option<DbConn>>>;

/// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub db: SharedDbConn,
    pub k8s_client: SharedK8sClient,
    pub catalog: SharedCatalog,
    pub audit: AuditService,
    pub notification: NotificationService,
    pub proxy: ProxyService,
    pub endpoint_cache: EndpointCache,
    pub network_metrics_cache: NetworkMetricsCache,
    pub network_metrics_tx: NetworkMetricsBroadcast,
    pub bootstrap_tx: BootstrapBroadcast,
}

impl AppState {
    pub fn new(
        db: Option<DbConn>,
        k8s_client: SharedK8sClient,
        catalog: SharedCatalog,
        audit: AuditService,
        notification: NotificationService,
    ) -> Self {
        // Create broadcast channel for network metrics (capacity of 16 messages)
        let (network_metrics_tx, _) = broadcast::channel(16);
        // Create broadcast channel for bootstrap progress (capacity of 32 messages)
        let (bootstrap_tx, _) = broadcast::channel(32);

        Self {
            db: Arc::new(RwLock::new(db)),
            k8s_client,
            catalog,
            audit,
            notification,
            proxy: ProxyService::new(),
            endpoint_cache: EndpointCache::new(60), // Cache endpoints for 60 seconds
            network_metrics_cache: NetworkMetricsCache::new(),
            network_metrics_tx,
            bootstrap_tx,
        }
    }

    /// Set the database connection after PostgreSQL is installed
    pub async fn set_db(&self, db: DbConn) {
        let mut db_guard = self.db.write().await;
        *db_guard = Some(db);
    }

    /// Get the database connection (returns error if not connected)
    pub async fn get_db(&self) -> crate::error::Result<DbConn> {
        let db_guard = self.db.read().await;
        db_guard.clone().ok_or_else(|| {
            crate::error::AppError::ServiceUnavailable("Database not connected. Please complete setup.".to_string())
        })
    }

    /// Check if database is connected
    pub async fn is_db_connected(&self) -> bool {
        let db_guard = self.db.read().await;
        db_guard.is_some()
    }
}
