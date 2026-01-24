import { useState, useEffect, useCallback, useRef } from 'react'
import { logsApi, LokiStream, LokiLogEntry } from '../api/logs'
import { AppIcon } from '../components/AppIcon'
import {
  Search,
  RefreshCw,
  Clock,
  Filter,
  ChevronDown,
  AlertCircle,
  FileText,
  Play,
  Pause,
  Check
} from 'lucide-react'

// App display names (namespace -> display name)
const APP_LABELS: Record<string, string> = {
  sonarr: 'Sonarr',
  radarr: 'Radarr',
  qbittorrent: 'qBittorrent',
  transmission: 'Transmission',
  deluge: 'Deluge',
  rutorrent: 'ruTorrent',
  jellyfin: 'Jellyfin',
  jellyseerr: 'Jellyseerr',
  jackett: 'Jackett',
  sabnzbd: 'SABnzbd',
  prometheus: 'Prometheus',
  loki: 'Loki',
  promtail: 'Promtail',
  grafana: 'Grafana',
  'kubarr-system': 'Kubarr',
}

// Get display label for app
function getAppLabel(app: string): string {
  return APP_LABELS[app] || app.charAt(0).toUpperCase() + app.slice(1)
}

// Time range options
const TIME_RANGES = [
  { label: 'Last 15 minutes', value: '15m', ms: 15 * 60 * 1000 },
  { label: 'Last 1 hour', value: '1h', ms: 60 * 60 * 1000 },
  { label: 'Last 3 hours', value: '3h', ms: 3 * 60 * 60 * 1000 },
  { label: 'Last 6 hours', value: '6h', ms: 6 * 60 * 60 * 1000 },
  { label: 'Last 12 hours', value: '12h', ms: 12 * 60 * 60 * 1000 },
  { label: 'Last 24 hours', value: '24h', ms: 24 * 60 * 60 * 1000 },
  { label: 'Last 7 days', value: '7d', ms: 7 * 24 * 60 * 60 * 1000 },
]

// Format timestamp for display
function formatTimestamp(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleString('en-US', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

// Flatten streams into a single sorted list of log entries
function flattenStreams(streams: LokiStream[] | null | undefined): Array<LokiLogEntry & { labels: Record<string, string> }> {
  const entries: Array<LokiLogEntry & { labels: Record<string, string> }> = []

  if (!streams || !Array.isArray(streams)) {
    return entries
  }

  for (const stream of streams) {
    if (!stream || !Array.isArray(stream.entries)) continue
    for (const entry of stream.entries) {
      entries.push({
        ...entry,
        labels: stream.labels || {},
      })
    }
  }

  // Sort by timestamp (newest first)
  entries.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())

  return entries
}

export default function LogsPage() {
  const [apps, setApps] = useState<string[]>([])
  const [selectedApps, setSelectedApps] = useState<string[]>([])
  const [timeRange, setTimeRange] = useState(TIME_RANGES[1]) // Default to 1 hour
  const [searchText, setSearchText] = useState('')
  const [streams, setStreams] = useState<LokiStream[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [showAppDropdown, setShowAppDropdown] = useState(false)
  const [showTimeDropdown, setShowTimeDropdown] = useState(false)

  const logsContainerRef = useRef<HTMLDivElement>(null)
  const autoRefreshRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Load apps (namespaces) on mount
  useEffect(() => {
    const loadApps = async () => {
      try {
        const ns = await logsApi.getNamespaces()
        const appList = Array.isArray(ns) ? ns : []
        setApps(appList)
        // Select all apps by default
        setSelectedApps(appList)
      } catch (err) {
        console.error('Failed to load apps:', err)
        setError('Failed to connect to Loki. Make sure Loki is running.')
        setApps([])
        setSelectedApps([])
      }
    }
    loadApps()
  }, [])

  // Build LogQL query
  const buildQuery = useCallback(() => {
    let query = ''

    if (selectedApps.length === 0 || selectedApps.length === apps.length) {
      // All apps or none selected - query all
      query = '{namespace=~".+"}'
    } else if (selectedApps.length === 1) {
      query = `{namespace="${selectedApps[0]}"}`
    } else {
      const appRegex = selectedApps.join('|')
      query = `{namespace=~"${appRegex}"}`
    }

    // Add search filter if provided
    if (searchText.trim()) {
      query += ` |~ "(?i)${searchText.trim()}"`
    }

    return query
  }, [selectedApps, apps.length, searchText])

  // Fetch logs
  const fetchLogs = useCallback(async () => {
    if (apps.length === 0) {
      return
    }

    setLoading(true)
    setError(null)

    try {
      const now = Date.now()
      const start = String((now - timeRange.ms) * 1e6)
      const end = String(now * 1e6)

      const response = await logsApi.queryLogs({
        query: buildQuery(),
        start,
        end,
        limit: 2000,
        direction: 'backward',
      })

      setStreams(response?.streams || [])
    } catch (err: any) {
      setError(err.response?.data?.detail || err.message || 'Failed to fetch logs')
      setStreams([])
    } finally {
      setLoading(false)
    }
  }, [buildQuery, timeRange.ms, apps.length])

  // Initial fetch and when filters change
  useEffect(() => {
    if (selectedApps.length > 0) {
      fetchLogs()
    }
  }, [fetchLogs, selectedApps.length])

  // Auto-refresh
  useEffect(() => {
    if (autoRefresh) {
      autoRefreshRef.current = setInterval(fetchLogs, 5000)
    } else if (autoRefreshRef.current) {
      clearInterval(autoRefreshRef.current)
      autoRefreshRef.current = null
    }

    return () => {
      if (autoRefreshRef.current) {
        clearInterval(autoRefreshRef.current)
      }
    }
  }, [autoRefresh, fetchLogs])

  // Toggle app selection
  const toggleApp = (app: string) => {
    setSelectedApps(prev =>
      prev.includes(app)
        ? prev.filter(a => a !== app)
        : [...prev, app]
    )
  }

  // Select/deselect all apps
  const toggleAllApps = () => {
    if (selectedApps.length === apps.length) {
      setSelectedApps([])
    } else {
      setSelectedApps([...apps])
    }
  }

  // Flatten and filter entries
  const entries = flattenStreams(streams)

  // Get selected apps display text
  const getSelectedAppsText = () => {
    if (selectedApps.length === 0) return 'No apps selected'
    if (selectedApps.length === apps.length) return 'All apps'
    if (selectedApps.length === 1) {
      return getAppLabel(selectedApps[0])
    }
    return `${selectedApps.length} apps`
  }

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <FileText className="text-blue-400" />
          Logs
        </h1>
        <p className="text-gray-400 mt-1">View application logs from Loki</p>
      </div>

      {/* Filters Bar */}
      <div className="flex flex-wrap items-center gap-3 mb-4">
        {/* App Filter */}
        <div className="relative">
          <button
            onClick={() => setShowAppDropdown(!showAppDropdown)}
            className="flex items-center gap-2 px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-750 transition-colors"
          >
            <Filter size={16} />
            <span>{getSelectedAppsText()}</span>
            <ChevronDown size={16} />
          </button>

          {showAppDropdown && (
            <div className="absolute top-full left-0 mt-1 w-72 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-50 max-h-96 overflow-y-auto">
              <div className="p-2 border-b border-gray-700">
                <button
                  onClick={toggleAllApps}
                  className="w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700 rounded transition-colors"
                >
                  <div className={`w-5 h-5 rounded border flex items-center justify-center ${
                    selectedApps.length === apps.length
                      ? 'bg-blue-600 border-blue-600'
                      : 'border-gray-600'
                  }`}>
                    {selectedApps.length === apps.length && <Check size={14} />}
                  </div>
                  <span className="font-medium">
                    {selectedApps.length === apps.length ? 'Deselect All' : 'Select All'}
                  </span>
                </button>
              </div>
              <div className="p-2 space-y-1">
                {apps.map(app => {
                  const isSelected = selectedApps.includes(app)
                  return (
                    <button
                      key={app}
                      onClick={() => toggleApp(app)}
                      className="w-full flex items-center gap-3 px-3 py-2 hover:bg-gray-700 rounded transition-colors text-left"
                    >
                      <div className={`w-5 h-5 rounded border flex items-center justify-center ${
                        isSelected ? 'bg-blue-600 border-blue-600' : 'border-gray-600'
                      }`}>
                        {isSelected && <Check size={14} />}
                      </div>
                      <AppIcon appName={app} size={32} />
                      <span className="flex-1 font-medium">{getAppLabel(app)}</span>
                    </button>
                  )
                })}
              </div>
            </div>
          )}
        </div>

        {/* Time Range */}
        <div className="relative">
          <button
            onClick={() => setShowTimeDropdown(!showTimeDropdown)}
            className="flex items-center gap-2 px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-750 transition-colors"
          >
            <Clock size={16} />
            <span>{timeRange.label}</span>
            <ChevronDown size={16} />
          </button>

          {showTimeDropdown && (
            <div className="absolute top-full left-0 mt-1 w-48 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-50">
              {TIME_RANGES.map(range => (
                <button
                  key={range.value}
                  onClick={() => {
                    setTimeRange(range)
                    setShowTimeDropdown(false)
                  }}
                  className={`w-full text-left px-4 py-2 text-sm hover:bg-gray-700 transition-colors first:rounded-t-lg last:rounded-b-lg ${
                    timeRange.value === range.value ? 'bg-gray-700 text-blue-400' : ''
                  }`}
                >
                  {range.label}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Search */}
        <div className="flex-1 min-w-[200px] max-w-md relative">
          <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
          <input
            type="text"
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && fetchLogs()}
            placeholder="Search logs... (press Enter)"
            className="w-full pl-10 pr-4 py-2 bg-gray-800 border border-gray-700 rounded-lg focus:outline-none focus:border-blue-500 transition-colors"
          />
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg transition-colors ${
              autoRefresh
                ? 'bg-green-600 hover:bg-green-700'
                : 'bg-gray-800 border border-gray-700 hover:bg-gray-750'
            }`}
            title={autoRefresh ? 'Stop auto-refresh' : 'Start auto-refresh (5s)'}
          >
            {autoRefresh ? <Pause size={16} /> : <Play size={16} />}
            <span className="hidden sm:inline">{autoRefresh ? 'Live' : 'Auto'}</span>
          </button>

          <button
            onClick={fetchLogs}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors disabled:opacity-50"
          >
            <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
            <span className="hidden sm:inline">Refresh</span>
          </button>
        </div>
      </div>

      {/* Close dropdowns on click outside */}
      {(showAppDropdown || showTimeDropdown) && (
        <div
          className="fixed inset-0 z-40"
          onClick={() => {
            setShowAppDropdown(false)
            setShowTimeDropdown(false)
          }}
        />
      )}

      {/* Error */}
      {error && (
        <div className="mb-4 p-4 bg-red-900/30 border border-red-700 rounded-lg flex items-center gap-3">
          <AlertCircle className="text-red-400 flex-shrink-0" />
          <span className="text-red-300">{error}</span>
        </div>
      )}

      {/* Stats */}
      <div className="mb-2 text-sm text-gray-400">
        {loading ? (
          <span>Loading...</span>
        ) : (
          <span>{entries.length.toLocaleString()} log entries</span>
        )}
        {autoRefresh && <span className="ml-2 text-green-400">Auto-refreshing every 5s</span>}
      </div>

      {/* Log Viewer */}
      <div
        ref={logsContainerRef}
        className="flex-1 bg-gray-950 border border-gray-800 rounded-lg overflow-auto font-mono text-sm"
      >
        {entries.length === 0 && !loading ? (
          <div className="flex items-center justify-center h-full text-gray-500">
            <div className="text-center">
              <FileText size={48} className="mx-auto mb-4 opacity-50" />
              <p>No logs found</p>
              <p className="text-sm mt-1">Try adjusting your filters or time range</p>
            </div>
          </div>
        ) : (
          <table className="w-full">
            <tbody>
              {entries.map((entry, index) => {
                const appName = entry.labels.namespace || 'unknown'
                return (
                  <tr
                    key={`${entry.timestamp}-${index}`}
                    className="hover:bg-gray-900/50 border-b border-gray-800/50"
                  >
                    <td className="px-3 py-1 text-gray-500 whitespace-nowrap align-top text-xs">
                      {formatTimestamp(entry.timestamp)}
                    </td>
                    <td className="px-2 py-1 whitespace-nowrap align-top">
                      <div className="inline-flex items-center gap-1.5 px-1.5 py-0.5 text-xs rounded bg-gray-800">
                        <AppIcon appName={appName} size={18} />
                        <span>{getAppLabel(appName)}</span>
                      </div>
                    </td>
                    <td className="px-2 py-1 text-gray-400 whitespace-nowrap align-top text-xs max-w-[150px] truncate" title={entry.labels.pod || ''}>
                      {entry.labels.container || entry.labels.pod?.split('-').slice(0, -2).join('-') || ''}
                    </td>
                    <td className="px-3 py-1 text-gray-200 whitespace-pre-wrap break-all">
                      {entry.line}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
