import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { monitoringApi, TimeSeriesPoint } from '../api/monitoring'
import { logsApi } from '../api/logs'
import { AppIcon } from '../components/AppIcon'
import {
  Activity,
  Cpu,
  HardDrive,
  Server,
  Box,
  RefreshCw,
  AlertCircle,
  TrendingUp,
  Gauge,
  X,
  Clock,
  CheckCircle,
  XCircle,
  RotateCcw,
  FileText
} from 'lucide-react'

// Format bytes to human readable
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`
}

// Format CPU cores to millicores or cores
function formatCpu(cores: number): string {
  if (cores < 0.001) return '< 1m'
  if (cores < 1) return `${Math.round(cores * 1000)}m`
  return `${cores.toFixed(2)} cores`
}

// Format timestamp
function formatTime(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
  })
}

// Progress bar component
function ProgressBar({ value, max, color = 'blue' }: { value: number; max: number; color?: string }) {
  const percentage = max > 0 ? Math.min((value / max) * 100, 100) : 0
  const colorClasses: Record<string, string> = {
    blue: 'bg-blue-500',
    green: 'bg-green-500',
    yellow: 'bg-yellow-500',
    red: 'bg-red-500',
  }

  return (
    <div className="w-full bg-gray-700 rounded-full h-2">
      <div
        className={`h-2 rounded-full transition-all duration-300 ${colorClasses[color] || colorClasses.blue}`}
        style={{ width: `${percentage}%` }}
      />
    </div>
  )
}

// Simple line chart component
function SimpleChart({
  data,
  color = 'blue',
  height = 120,
  formatValue,
}: {
  data: TimeSeriesPoint[];
  color?: string;
  height?: number;
  formatValue: (v: number) => string;
}) {
  if (!data || data.length === 0) {
    return (
      <div className="flex items-center justify-center text-gray-500" style={{ height }}>
        No data available
      </div>
    )
  }

  const values = data.map(d => d.value)
  const maxValue = Math.max(...values) || 1
  const minValue = Math.min(...values)

  const colorClasses: Record<string, string> = {
    blue: 'stroke-blue-500 fill-blue-500/20',
    green: 'stroke-green-500 fill-green-500/20',
  }

  // Generate path
  const width = 100
  const pathPoints = data.map((d, i) => {
    const x = (i / (data.length - 1)) * width
    const y = height - ((d.value - minValue) / (maxValue - minValue || 1)) * (height - 20) - 10
    return `${x},${y}`
  })

  const linePath = `M ${pathPoints.join(' L ')}`
  const areaPath = `${linePath} L ${width},${height} L 0,${height} Z`

  // Get first and last timestamps for x-axis labels
  const startTime = formatTime(data[0].timestamp)
  const endTime = formatTime(data[data.length - 1].timestamp)

  return (
    <div className="relative" style={{ height: height + 20 }}>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className="w-full"
        style={{ height }}
      >
        <path d={areaPath} className={colorClasses[color]} fillOpacity="0.2" />
        <path d={linePath} className={colorClasses[color]} fill="none" strokeWidth="1.5" />
      </svg>
      <div className="flex justify-between text-xs text-gray-500 mt-1">
        <span>{startTime}</span>
        <span className="text-gray-400">{formatValue(values[values.length - 1])}</span>
        <span>{endTime}</span>
      </div>
    </div>
  )
}

// Cluster stats card
function ClusterStatsCard({
  icon: Icon,
  label,
  value,
  subValue,
  color = 'blue'
}: {
  icon: React.ElementType;
  label: string;
  value: string;
  subValue?: string;
  color?: string;
}) {
  const colorClasses: Record<string, string> = {
    blue: 'text-blue-400',
    green: 'text-green-400',
    yellow: 'text-yellow-400',
    purple: 'text-purple-400',
  }

  return (
    <div className="bg-gray-800 rounded-lg p-6">
      <div className="flex items-center gap-3 mb-2">
        <Icon className={colorClasses[color]} size={24} />
        <span className="text-gray-400 text-sm">{label}</span>
      </div>
      <div className="text-2xl font-bold">{value}</div>
      {subValue && <div className="text-sm text-gray-500 mt-1">{subValue}</div>}
    </div>
  )
}

// App Detail Modal
function AppDetailModal({
  appName,
  onClose,
}: {
  appName: string;
  onClose: () => void;
}) {
  const [duration, setDuration] = useState('1h')
  const [activeTab, setActiveTab] = useState<'metrics' | 'pods' | 'logs'>('metrics')

  const { data: detailMetrics, isLoading } = useQuery({
    queryKey: ['monitoring', 'app', appName, duration],
    queryFn: () => monitoringApi.getAppDetailMetrics(appName, duration),
    refetchInterval: 30000,
  })

  const { data: logsData, isLoading: logsLoading } = useQuery({
    queryKey: ['logs', appName],
    queryFn: async () => {
      const now = Date.now()
      const start = String((now - 60 * 60 * 1000) * 1e6) // Last hour
      const end = String(now * 1e6)
      return logsApi.queryLogs({
        query: `{namespace="${appName}"}`,
        start,
        end,
        limit: 100,
        direction: 'backward',
      })
    },
    enabled: activeTab === 'logs',
  })

  const durations = [
    { label: '15m', value: '15m' },
    { label: '1h', value: '1h' },
    { label: '3h', value: '3h' },
    { label: '6h', value: '6h' },
    { label: '12h', value: '12h' },
    { label: '24h', value: '24h' },
  ]

  // Flatten logs
  const logEntries = logsData?.streams?.flatMap(stream =>
    stream.entries.map(entry => ({
      ...entry,
      labels: stream.labels,
    }))
  ).sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()) || []

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50 p-4">
      <div className="bg-gray-800 rounded-xl w-full max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-gray-700">
          <div className="flex items-center gap-4">
            <AppIcon appName={appName} size={48} />
            <div>
              <h2 className="text-2xl font-bold capitalize">{appName}</h2>
              <p className="text-gray-400 text-sm">Detailed metrics and information</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-gray-700 rounded-lg transition-colors"
          >
            <X size={24} />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-gray-700">
          <button
            onClick={() => setActiveTab('metrics')}
            className={`px-6 py-3 font-medium transition-colors ${
              activeTab === 'metrics'
                ? 'text-blue-400 border-b-2 border-blue-400'
                : 'text-gray-400 hover:text-white'
            }`}
          >
            <div className="flex items-center gap-2">
              <TrendingUp size={18} />
              Metrics
            </div>
          </button>
          <button
            onClick={() => setActiveTab('pods')}
            className={`px-6 py-3 font-medium transition-colors ${
              activeTab === 'pods'
                ? 'text-blue-400 border-b-2 border-blue-400'
                : 'text-gray-400 hover:text-white'
            }`}
          >
            <div className="flex items-center gap-2">
              <Box size={18} />
              Pods ({detailMetrics?.pods?.length || 0})
            </div>
          </button>
          <button
            onClick={() => setActiveTab('logs')}
            className={`px-6 py-3 font-medium transition-colors ${
              activeTab === 'logs'
                ? 'text-blue-400 border-b-2 border-blue-400'
                : 'text-gray-400 hover:text-white'
            }`}
          >
            <div className="flex items-center gap-2">
              <FileText size={18} />
              Logs
            </div>
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-6">
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
            </div>
          ) : activeTab === 'metrics' ? (
            <div className="space-y-6">
              {/* Duration selector */}
              <div className="flex items-center gap-2">
                <Clock size={16} className="text-gray-400" />
                <span className="text-sm text-gray-400">Time range:</span>
                <div className="flex gap-1">
                  {durations.map(d => (
                    <button
                      key={d.value}
                      onClick={() => setDuration(d.value)}
                      className={`px-3 py-1 text-sm rounded transition-colors ${
                        duration === d.value
                          ? 'bg-blue-600 text-white'
                          : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
                      }`}
                    >
                      {d.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Current stats */}
              <div className="grid grid-cols-2 gap-4">
                <div className="bg-gray-900 rounded-lg p-4">
                  <div className="flex items-center gap-2 text-gray-400 mb-2">
                    <Cpu size={16} />
                    <span className="text-sm">CPU Usage</span>
                  </div>
                  <div className="text-2xl font-bold text-blue-400">
                    {formatCpu(detailMetrics?.historical?.cpu_usage_cores || 0)}
                  </div>
                </div>
                <div className="bg-gray-900 rounded-lg p-4">
                  <div className="flex items-center gap-2 text-gray-400 mb-2">
                    <HardDrive size={16} />
                    <span className="text-sm">Memory Usage</span>
                  </div>
                  <div className="text-2xl font-bold text-green-400">
                    {formatBytes(detailMetrics?.historical?.memory_usage_bytes || 0)}
                  </div>
                </div>
              </div>

              {/* Charts */}
              <div className="space-y-6">
                <div className="bg-gray-900 rounded-lg p-4">
                  <h3 className="text-sm font-medium text-gray-400 mb-4 flex items-center gap-2">
                    <Cpu size={16} className="text-blue-400" />
                    CPU Usage History
                  </h3>
                  <SimpleChart
                    data={detailMetrics?.historical?.cpu_series || []}
                    color="blue"
                    height={100}
                    formatValue={formatCpu}
                  />
                </div>

                <div className="bg-gray-900 rounded-lg p-4">
                  <h3 className="text-sm font-medium text-gray-400 mb-4 flex items-center gap-2">
                    <HardDrive size={16} className="text-green-400" />
                    Memory Usage History
                  </h3>
                  <SimpleChart
                    data={detailMetrics?.historical?.memory_series || []}
                    color="green"
                    height={100}
                    formatValue={formatBytes}
                  />
                </div>
              </div>
            </div>
          ) : activeTab === 'pods' ? (
            <div className="space-y-4">
              {detailMetrics?.pods?.length === 0 ? (
                <div className="text-center py-12 text-gray-400">
                  <Box size={48} className="mx-auto mb-4 opacity-50" />
                  <p>No pods found</p>
                </div>
              ) : (
                <div className="bg-gray-900 rounded-lg overflow-hidden">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b border-gray-700">
                        <th className="text-left px-4 py-3 text-sm font-medium text-gray-400">Pod</th>
                        <th className="text-left px-4 py-3 text-sm font-medium text-gray-400">Status</th>
                        <th className="text-left px-4 py-3 text-sm font-medium text-gray-400">Ready</th>
                        <th className="text-left px-4 py-3 text-sm font-medium text-gray-400">Restarts</th>
                        <th className="text-left px-4 py-3 text-sm font-medium text-gray-400">Age</th>
                      </tr>
                    </thead>
                    <tbody>
                      {detailMetrics?.pods?.map((pod) => (
                        <tr key={pod.name} className="border-b border-gray-800 hover:bg-gray-800/50">
                          <td className="px-4 py-3">
                            <div className="font-mono text-sm">{pod.name}</div>
                            <div className="text-xs text-gray-500">{pod.ip || 'No IP'}</div>
                          </td>
                          <td className="px-4 py-3">
                            <span className={`inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-medium ${
                              pod.status === 'Running'
                                ? 'bg-green-900/50 text-green-400'
                                : pod.status === 'Pending'
                                ? 'bg-yellow-900/50 text-yellow-400'
                                : 'bg-red-900/50 text-red-400'
                            }`}>
                              {pod.status}
                            </span>
                          </td>
                          <td className="px-4 py-3">
                            {pod.ready ? (
                              <CheckCircle size={18} className="text-green-400" />
                            ) : (
                              <XCircle size={18} className="text-red-400" />
                            )}
                          </td>
                          <td className="px-4 py-3">
                            <span className={`flex items-center gap-1 ${pod.restarts > 0 ? 'text-yellow-400' : 'text-gray-400'}`}>
                              {pod.restarts > 0 && <RotateCcw size={14} />}
                              {pod.restarts}
                            </span>
                          </td>
                          <td className="px-4 py-3 text-gray-400 text-sm">{pod.age}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          ) : (
            <div className="space-y-4">
              {logsLoading ? (
                <div className="flex items-center justify-center py-12">
                  <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
                </div>
              ) : logEntries.length === 0 ? (
                <div className="text-center py-12 text-gray-400">
                  <FileText size={48} className="mx-auto mb-4 opacity-50" />
                  <p>No logs found</p>
                  <p className="text-sm mt-1">Logs from the last hour will appear here</p>
                </div>
              ) : (
                <div className="bg-gray-900 rounded-lg overflow-hidden">
                  <div className="max-h-96 overflow-y-auto font-mono text-sm">
                    {logEntries.slice(0, 100).map((entry, i) => (
                      <div
                        key={i}
                        className="px-4 py-2 border-b border-gray-800 hover:bg-gray-800/50"
                      >
                        <div className="flex items-start gap-3">
                          <span className="text-gray-500 text-xs whitespace-nowrap">
                            {new Date(entry.timestamp).toLocaleTimeString()}
                          </span>
                          <span className="text-gray-300 break-all">{entry.line}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default function MonitoringPage() {
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [selectedApp, setSelectedApp] = useState<string | null>(null)

  // Check if Prometheus is available
  const { data: prometheusStatus, isLoading: prometheusLoading } = useQuery({
    queryKey: ['monitoring', 'prometheus', 'available'],
    queryFn: monitoringApi.checkPrometheusAvailable,
    refetchInterval: 30000,
  })

  // Get cluster metrics
  const {
    data: clusterMetrics,
    isLoading: clusterLoading,
    refetch: refetchCluster,
  } = useQuery({
    queryKey: ['monitoring', 'prometheus', 'cluster'],
    queryFn: monitoringApi.getClusterMetrics,
    refetchInterval: autoRefresh ? 10000 : false,
    enabled: prometheusStatus?.available,
  })

  // Get app metrics
  const {
    data: appMetrics,
    isLoading: appsLoading,
    refetch: refetchApps,
  } = useQuery({
    queryKey: ['monitoring', 'prometheus', 'apps'],
    queryFn: monitoringApi.getAppMetricsFromPrometheus,
    refetchInterval: autoRefresh ? 10000 : false,
    enabled: prometheusStatus?.available,
  })

  const handleRefresh = () => {
    refetchCluster()
    refetchApps()
  }

  // Sort apps by memory usage (descending)
  const sortedApps = [...(appMetrics || [])].sort(
    (a, b) => b.memory_usage_bytes - a.memory_usage_bytes
  )

  // Calculate totals for apps
  const totalAppMemory = sortedApps.reduce((sum, app) => sum + app.memory_usage_bytes, 0)
  const totalAppCpu = sortedApps.reduce((sum, app) => sum + app.cpu_usage_cores, 0)

  if (prometheusLoading) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
      </div>
    )
  }

  if (!prometheusStatus?.available) {
    return (
      <div className="flex flex-col items-center justify-center h-[60vh] text-center">
        <AlertCircle size={64} className="text-yellow-500 mb-4" />
        <h2 className="text-2xl font-bold mb-2">Prometheus Not Available</h2>
        <p className="text-gray-400 max-w-md">
          {prometheusStatus?.message || 'Cannot connect to Prometheus. Make sure it is installed and running.'}
        </p>
        <p className="text-gray-500 text-sm mt-4">
          Install Prometheus from the Apps page to enable monitoring.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-8">
      {/* App Detail Modal */}
      {selectedApp && (
        <AppDetailModal appName={selectedApp} onClose={() => setSelectedApp(null)} />
      )}

      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            <Activity className="text-blue-400" />
            Monitoring
          </h1>
          <p className="text-gray-400 mt-1">Resource usage metrics from Prometheus</p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg transition-colors ${
              autoRefresh
                ? 'bg-green-600 hover:bg-green-700'
                : 'bg-gray-700 hover:bg-gray-600'
            }`}
          >
            <Gauge size={18} />
            {autoRefresh ? 'Live' : 'Paused'}
          </button>
          <button
            onClick={handleRefresh}
            disabled={clusterLoading || appsLoading}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors disabled:opacity-50"
          >
            <RefreshCw size={18} className={clusterLoading || appsLoading ? 'animate-spin' : ''} />
            Refresh
          </button>
        </div>
      </div>

      {/* Cluster Overview */}
      <div>
        <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
          <Server size={20} />
          Cluster Overview
        </h2>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <ClusterStatsCard
            icon={Cpu}
            label="CPU Usage"
            value={`${clusterMetrics?.cpu_usage_percent?.toFixed(1) || 0}%`}
            subValue={`${formatCpu(clusterMetrics?.used_cpu_cores || 0)} / ${clusterMetrics?.total_cpu_cores || 0} cores`}
            color="blue"
          />
          <ClusterStatsCard
            icon={HardDrive}
            label="Memory Usage"
            value={`${clusterMetrics?.memory_usage_percent?.toFixed(1) || 0}%`}
            subValue={`${formatBytes(clusterMetrics?.used_memory_bytes || 0)} / ${formatBytes(clusterMetrics?.total_memory_bytes || 0)}`}
            color="green"
          />
          <ClusterStatsCard
            icon={Box}
            label="Containers"
            value={String(clusterMetrics?.container_count || 0)}
            subValue="Running containers"
            color="purple"
          />
          <ClusterStatsCard
            icon={Server}
            label="Pods"
            value={String(clusterMetrics?.pod_count || 0)}
            subValue="Active pods"
            color="yellow"
          />
        </div>

        {/* Cluster usage bars */}
        <div className="mt-6 grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="bg-gray-800 rounded-lg p-4">
            <div className="flex justify-between mb-2">
              <span className="text-sm text-gray-400">CPU</span>
              <span className="text-sm">{clusterMetrics?.cpu_usage_percent?.toFixed(1) || 0}%</span>
            </div>
            <ProgressBar
              value={clusterMetrics?.used_cpu_cores || 0}
              max={clusterMetrics?.total_cpu_cores || 1}
              color="blue"
            />
          </div>
          <div className="bg-gray-800 rounded-lg p-4">
            <div className="flex justify-between mb-2">
              <span className="text-sm text-gray-400">Memory</span>
              <span className="text-sm">{clusterMetrics?.memory_usage_percent?.toFixed(1) || 0}%</span>
            </div>
            <ProgressBar
              value={clusterMetrics?.used_memory_bytes || 0}
              max={clusterMetrics?.total_memory_bytes || 1}
              color="green"
            />
          </div>
        </div>
      </div>

      {/* Per-App Metrics */}
      <div>
        <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
          <TrendingUp size={20} />
          App Resource Usage
          <span className="text-sm font-normal text-gray-400 ml-2">Click an app for details</span>
        </h2>

        {appsLoading ? (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
          </div>
        ) : sortedApps.length === 0 ? (
          <div className="bg-gray-800 rounded-lg p-8 text-center text-gray-400">
            <Box size={48} className="mx-auto mb-4 opacity-50" />
            <p>No app metrics available</p>
            <p className="text-sm mt-1">Install some apps to see their resource usage</p>
          </div>
        ) : (
          <div className="bg-gray-800 rounded-lg overflow-hidden">
            <table className="w-full">
              <thead>
                <tr className="border-b border-gray-700">
                  <th className="text-left px-6 py-4 text-sm font-medium text-gray-400">App</th>
                  <th className="text-right px-6 py-4 text-sm font-medium text-gray-400">CPU</th>
                  <th className="text-right px-6 py-4 text-sm font-medium text-gray-400">Memory</th>
                  <th className="text-left px-6 py-4 text-sm font-medium text-gray-400 w-1/3">Usage</th>
                </tr>
              </thead>
              <tbody>
                {sortedApps.map((app) => {
                  const memoryPercent = totalAppMemory > 0
                    ? (app.memory_usage_bytes / totalAppMemory) * 100
                    : 0

                  return (
                    <tr
                      key={app.namespace}
                      onClick={() => setSelectedApp(app.app_name)}
                      className="border-b border-gray-700/50 hover:bg-gray-700/30 transition-colors cursor-pointer"
                    >
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-3">
                          <AppIcon appName={app.app_name} size={32} />
                          <div>
                            <div className="font-medium capitalize">{app.app_name}</div>
                            <div className="text-xs text-gray-500">{app.namespace}</div>
                          </div>
                        </div>
                      </td>
                      <td className="text-right px-6 py-4">
                        <span className="text-blue-400 font-mono">
                          {formatCpu(app.cpu_usage_cores)}
                        </span>
                      </td>
                      <td className="text-right px-6 py-4">
                        <span className="text-green-400 font-mono">
                          {formatBytes(app.memory_usage_bytes)}
                        </span>
                      </td>
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-3">
                          <div className="flex-1">
                            <ProgressBar
                              value={app.memory_usage_bytes}
                              max={totalAppMemory}
                              color={memoryPercent > 50 ? 'yellow' : 'green'}
                            />
                          </div>
                          <span className="text-xs text-gray-500 w-12 text-right">
                            {memoryPercent.toFixed(1)}%
                          </span>
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
              <tfoot>
                <tr className="bg-gray-700/30">
                  <td className="px-6 py-4 font-medium">Total</td>
                  <td className="text-right px-6 py-4">
                    <span className="text-blue-400 font-mono font-medium">
                      {formatCpu(totalAppCpu)}
                    </span>
                  </td>
                  <td className="text-right px-6 py-4">
                    <span className="text-green-400 font-mono font-medium">
                      {formatBytes(totalAppMemory)}
                    </span>
                  </td>
                  <td className="px-6 py-4"></td>
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </div>

      {/* Auto-refresh indicator */}
      {autoRefresh && (
        <div className="text-center text-sm text-gray-500">
          Auto-refreshing every 10 seconds
        </div>
      )}
    </div>
  )
}
