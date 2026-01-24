import { AppIcon } from '../components/AppIcon'
import { useMonitoring } from '../contexts/MonitoringContext'
import { Cpu, MemoryStick, ArrowDownToLine, ArrowUpFromLine, Server, Container, HardDrive, Activity, AlertCircle } from 'lucide-react'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

function formatBytesPerSec(bytesPerSec: number): string {
  return formatBytes(bytesPerSec) + '/s'
}

export default function Dashboard() {
  const {
    clusterMetrics,
    metricsLoading,
    prometheusAvailable,
    catalog,
    installedApps: installedAppNames,
    appStatuses,
  } = useMonitoring()

  const installedApps = catalog.filter((app) => installedAppNames.includes(app.name))
  // Only show apps that can be opened (not hidden)
  const openableApps = installedApps.filter((app) => !app.is_hidden)

  const healthyApps = installedApps.filter((app) => {
    const status = appStatuses[app.name]
    return status?.healthy ?? false
  })

  return (
    <div className="space-y-8">
      {/* Status Panels */}
      <div>
        <h2 className="text-2xl font-bold mb-4">Overview</h2>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Installed Apps</div>
            <div className="text-3xl font-bold">{installedApps.length}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Healthy</div>
            <div className="text-3xl font-bold text-green-400">{healthyApps.length}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Unhealthy</div>
            <div className="text-3xl font-bold text-red-400">{installedApps.length - healthyApps.length}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Available</div>
            <div className="text-3xl font-bold text-blue-400">{(catalog?.length || 0) - installedApps.length}</div>
          </div>
        </div>
      </div>

      {/* System Resources */}
      <div>
        <div className="flex items-center gap-3 mb-4">
          <Activity className="w-6 h-6 text-blue-400" />
          <h2 className="text-2xl font-bold">System Resources</h2>
          {clusterMetrics && (
            <span className="text-xs text-gray-500 ml-auto">Live • Updates every 10s</span>
          )}
        </div>

        {/* Prometheus not available message */}
        {prometheusAvailable === false && (
          <div className="bg-gray-800 rounded-lg p-6 flex items-center gap-4">
            <AlertCircle className="w-8 h-8 text-yellow-500 flex-shrink-0" />
            <div>
              <p className="text-gray-300">Prometheus is not installed</p>
              <p className="text-sm text-gray-500">Install Prometheus from the Apps page to see detailed system metrics</p>
            </div>
            <a
              href="/apps"
              className="ml-auto bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
            >
              Install
            </a>
          </div>
        )}

        {/* Loading state */}
        {prometheusAvailable && metricsLoading && !clusterMetrics && (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {[...Array(6)].map((_, i) => (
              <div key={i} className="bg-gray-800 rounded-lg p-5 animate-pulse">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-9 h-9 bg-gray-700 rounded-lg" />
                  <div className="space-y-2">
                    <div className="h-3 w-20 bg-gray-700 rounded" />
                    <div className="h-5 w-16 bg-gray-700 rounded" />
                  </div>
                </div>
                <div className="h-2 bg-gray-700 rounded-full" />
              </div>
            ))}
          </div>
        )}

        {/* Metrics display */}
        {clusterMetrics && (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
            {/* CPU Usage */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-blue-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3 mb-3">
                <div className="p-2 bg-blue-500/20 rounded-lg group-hover:bg-blue-500/30 transition-colors">
                  <Cpu className="w-5 h-5 text-blue-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">CPU Usage</div>
                  <div className="text-xl font-semibold">
                    {clusterMetrics.cpu_usage_percent.toFixed(1)}%
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-lg font-bold text-blue-400">
                    {clusterMetrics.used_cpu_cores.toFixed(2)}
                  </div>
                  <div className="text-xs text-gray-500">
                    / {clusterMetrics.total_cpu_cores.toFixed(0)} cores
                  </div>
                </div>
              </div>
              <div className="w-full bg-gray-700 rounded-full h-2">
                <div
                  className={`h-2 rounded-full transition-all duration-500 ${
                    clusterMetrics.cpu_usage_percent > 80 ? 'bg-red-500' :
                    clusterMetrics.cpu_usage_percent > 60 ? 'bg-yellow-500' : 'bg-blue-500'
                  }`}
                  style={{ width: `${Math.min(clusterMetrics.cpu_usage_percent, 100)}%` }}
                />
              </div>
            </a>

            {/* Memory Usage */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-purple-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3 mb-3">
                <div className="p-2 bg-purple-500/20 rounded-lg group-hover:bg-purple-500/30 transition-colors">
                  <MemoryStick className="w-5 h-5 text-purple-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">Memory Usage</div>
                  <div className="text-xl font-semibold">
                    {clusterMetrics.memory_usage_percent.toFixed(1)}%
                  </div>
                </div>
                <div className="text-right">
                  <div className="text-lg font-bold text-purple-400">
                    {formatBytes(clusterMetrics.used_memory_bytes)}
                  </div>
                  <div className="text-xs text-gray-500">
                    / {formatBytes(clusterMetrics.total_memory_bytes)}
                  </div>
                </div>
              </div>
              <div className="w-full bg-gray-700 rounded-full h-2">
                <div
                  className={`h-2 rounded-full transition-all duration-500 ${
                    clusterMetrics.memory_usage_percent > 80 ? 'bg-red-500' :
                    clusterMetrics.memory_usage_percent > 60 ? 'bg-yellow-500' : 'bg-purple-500'
                  }`}
                  style={{ width: `${Math.min(clusterMetrics.memory_usage_percent, 100)}%` }}
                />
              </div>
            </a>

            {/* Storage Usage */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-rose-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3 mb-3">
                <div className="p-2 bg-rose-500/20 rounded-lg group-hover:bg-rose-500/30 transition-colors">
                  <HardDrive className="w-5 h-5 text-rose-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">Storage</div>
                  <div className="text-xl font-semibold">
                    {clusterMetrics.total_storage_bytes > 0
                      ? `${clusterMetrics.storage_usage_percent.toFixed(1)}%`
                      : 'N/A'}
                  </div>
                </div>
                {clusterMetrics.total_storage_bytes > 0 && (
                  <div className="text-right">
                    <div className="text-lg font-bold text-rose-400">
                      {formatBytes(clusterMetrics.used_storage_bytes)}
                    </div>
                    <div className="text-xs text-gray-500">
                      / {formatBytes(clusterMetrics.total_storage_bytes)}
                    </div>
                  </div>
                )}
              </div>
              <div className="w-full bg-gray-700 rounded-full h-2">
                <div
                  className={`h-2 rounded-full transition-all duration-500 ${
                    clusterMetrics.storage_usage_percent > 90 ? 'bg-red-500' :
                    clusterMetrics.storage_usage_percent > 70 ? 'bg-yellow-500' : 'bg-rose-500'
                  }`}
                  style={{ width: `${Math.min(clusterMetrics.storage_usage_percent, 100)}%` }}
                />
              </div>
            </a>

            {/* Network Traffic */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-green-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3 mb-3">
                <div className="p-2 bg-green-500/20 rounded-lg group-hover:bg-green-500/30 transition-colors">
                  <Activity className="w-5 h-5 text-green-400" />
                </div>
                <div>
                  <div className="text-gray-400 text-sm">Network I/O</div>
                  <div className="text-xl font-semibold">
                    {formatBytesPerSec(clusterMetrics.network_receive_bytes_per_sec + clusterMetrics.network_transmit_bytes_per_sec)}
                  </div>
                </div>
              </div>
              <div className="flex justify-between text-sm mt-2">
                <div className="flex items-center gap-2">
                  <ArrowDownToLine className="w-4 h-4 text-green-400" />
                  <span className="text-gray-500">In:</span>
                  <span className="text-green-400 font-medium">{formatBytesPerSec(clusterMetrics.network_receive_bytes_per_sec)}</span>
                </div>
                <div className="flex items-center gap-2">
                  <ArrowUpFromLine className="w-4 h-4 text-orange-400" />
                  <span className="text-gray-500">Out:</span>
                  <span className="text-orange-400 font-medium">{formatBytesPerSec(clusterMetrics.network_transmit_bytes_per_sec)}</span>
                </div>
              </div>
            </a>

            {/* Running Pods */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-cyan-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3">
                <div className="p-2 bg-cyan-500/20 rounded-lg group-hover:bg-cyan-500/30 transition-colors">
                  <Server className="w-5 h-5 text-cyan-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">Running Pods</div>
                  <div className="text-xl font-semibold">{clusterMetrics.pod_count}</div>
                </div>
                <div className="text-cyan-400/20">
                  <Server className="w-10 h-10" />
                </div>
              </div>
            </a>

            {/* Running Containers */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-yellow-500/50 transition-all cursor-pointer group"
            >
              <div className="flex items-center gap-3">
                <div className="p-2 bg-yellow-500/20 rounded-lg group-hover:bg-yellow-500/30 transition-colors">
                  <Container className="w-5 h-5 text-yellow-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">Running Containers</div>
                  <div className="text-xl font-semibold">{clusterMetrics.container_count}</div>
                </div>
                <div className="text-yellow-400/20">
                  <Container className="w-10 h-10" />
                </div>
              </div>
            </a>

            {/* Cluster Health Summary */}
            <a
              href="/monitoring"
              className="bg-gray-800 rounded-lg p-5 hover:bg-gray-750 hover:ring-1 hover:ring-emerald-500/50 transition-all cursor-pointer group md:col-span-2 lg:col-span-1 xl:col-span-2"
            >
              <div className="flex items-center gap-3">
                <div className="p-2 bg-emerald-500/20 rounded-lg group-hover:bg-emerald-500/30 transition-colors">
                  <Activity className="w-5 h-5 text-emerald-400" />
                </div>
                <div className="flex-1">
                  <div className="text-gray-400 text-sm">Cluster Status</div>
                  <div className={`text-xl font-semibold ${
                    healthyApps.length === installedApps.length ? 'text-emerald-400' : 'text-yellow-400'
                  }`}>
                    {healthyApps.length === installedApps.length ? 'All Systems Operational' : 'Degraded'}
                  </div>
                </div>
                <div className="flex flex-col items-end text-sm">
                  <span className="text-emerald-400 font-medium">{healthyApps.length} healthy</span>
                  {installedApps.length - healthyApps.length > 0 && (
                    <span className="text-red-400 font-medium">{installedApps.length - healthyApps.length} unhealthy</span>
                  )}
                </div>
              </div>
            </a>
          </div>
        )}
      </div>

      {/* App Grid - Launchpad Style */}
      {openableApps.length > 0 ? (
        <div>
          <h2 className="text-2xl font-bold mb-4">Installed Apps</h2>
          <div className="grid grid-cols-4 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-6">
            {openableApps.map((app) => {
              const status = appStatuses[app.name]
              const isHealthy = status?.healthy ?? false
              const showLoading = status?.loading ?? false
              const hasData = status !== undefined

              return (
                <a
                  key={app.name}
                  href={`/${app.name}/`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="group flex flex-col items-center gap-2 cursor-pointer"
                >
                  {/* Icon Container */}
                  <div className="relative">
                    <div className="transform group-hover:scale-110 transition-transform duration-200">
                      <AppIcon
                        appName={app.name}
                        size={64}
                        className="rounded-2xl shadow-lg group-hover:shadow-xl transition-shadow"
                      />
                    </div>

                    {/* Health Indicator - only show when we have data */}
                    {hasData && !showLoading && (
                      <div
                        className={`absolute -bottom-1 -right-1 w-4 h-4 rounded-full border-2 border-gray-900 ${
                          isHealthy ? 'bg-green-500' : 'bg-red-500'
                        }`}
                        title={isHealthy ? 'Running' : 'Not Ready'}
                      />
                    )}

                    {/* Loading indicator - show when loading or no data yet */}
                    {showLoading && (
                      <div className="absolute -bottom-1 -right-1 w-4 h-4 rounded-full border-2 border-gray-900 bg-gray-600 animate-pulse" />
                    )}
                  </div>

                  {/* App Name */}
                  <span className="text-xs text-gray-300 group-hover:text-white transition-colors text-center truncate max-w-[72px]">
                    {app.display_name}
                  </span>
                </a>
              )
            })}
          </div>
        </div>
      ) : (
        <div className="text-center py-12">
          <p className="text-gray-400 mb-4">No apps installed yet.</p>
          <a
            href="/apps"
            className="inline-block bg-blue-600 hover:bg-blue-500 text-white font-medium py-2 px-6 rounded-lg transition-colors"
          >
            Browse Apps
          </a>
        </div>
      )}
    </div>
  )
}
