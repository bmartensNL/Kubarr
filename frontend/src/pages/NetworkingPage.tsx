import { useState, useRef, useEffect, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { networkingApi, NetworkNode, NetworkTopology, formatBandwidth, formatPackets } from '../api/networking'
import { AppIcon } from '../components/AppIcon'
import {
  Network,
  RefreshCw,
  Gauge,
  AlertCircle,
  ArrowDownToLine,
  ArrowUpFromLine,
  Globe,
  Wifi,
  WifiOff,
  Zap,
} from 'lucide-react'

// ============================================================================
// Network Flow Visualization - Clean Tiered Layout
// ============================================================================

interface FlowVisualizationProps {
  topology: NetworkTopology
  width: number
  height: number
}

function NetworkFlowVisualization({ topology, width, height }: FlowVisualizationProps) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null)

  // Get nodes connected to a given node
  const getConnectedNodes = (nodeId: string): Set<string> => {
    const connected = new Set<string>([nodeId])
    topology.edges.forEach(edge => {
      if (edge.source === nodeId) connected.add(edge.target)
      if (edge.target === nodeId) connected.add(edge.source)
    })
    return connected
  }

  // Check if a node is connected to the hovered node
  const isNodeConnected = (nodeId: string): boolean => {
    if (!hoveredNode) return true // No hover = all visible
    return getConnectedNodes(hoveredNode).has(nodeId)
  }

  // Check if an edge is connected to the hovered node
  const isEdgeConnected = (source: string, target: string): boolean => {
    if (!hoveredNode) return true
    return source === hoveredNode || target === hoveredNode
  }

  if (topology.nodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-gray-500 dark:text-gray-400">
        <div className="text-center">
          <Network size={48} className="mx-auto mb-4 opacity-50" />
          <p>No network data available</p>
        </div>
      </div>
    )
  }

  // Categorize nodes into tiers - sort by ID for stable positions across updates
  const externalNode = topology.nodes.find(n => n.type === 'external')
  const appNodes = useMemo(() =>
    topology.nodes.filter(n => n.type === 'app').sort((a, b) => a.id.localeCompare(b.id)),
    [topology.nodes.map(n => n.id).join(',')]
  )
  const systemNodes = useMemo(() =>
    topology.nodes.filter(n => n.type === 'system' || n.type === 'monitoring').sort((a, b) => a.id.localeCompare(b.id)),
    [topology.nodes.map(n => n.id).join(',')]
  )

  // Layout constants
  const padding = 60
  const nodeWidth = 100 // Width of node card
  const nodeHeight = 100 // Height of node card including margins
  const minSpacing = 110 // Minimum vertical spacing between node centers

  // Calculate how many apps can fit per column
  const availableHeight = height - padding * 2
  const maxAppsPerColumn = Math.max(2, Math.floor(availableHeight / minSpacing))

  // Determine number of app columns needed
  const appColumns = appNodes.length > maxAppsPerColumn ? Math.ceil(appNodes.length / maxAppsPerColumn) : 1

  // Calculate horizontal positions based on number of app columns
  let tier1X: number, tier3X: number, appColumnXs: number[]

  if (appColumns === 1) {
    tier1X = padding + nodeWidth / 2
    tier3X = width - padding - nodeWidth / 2
    appColumnXs = [width / 2]
  } else {
    // Multiple columns of apps - spread them evenly
    tier1X = padding + nodeWidth / 2
    tier3X = width - padding - nodeWidth / 2
    const appAreaStart = tier1X + nodeWidth
    const appAreaEnd = tier3X - nodeWidth
    const appAreaWidth = appAreaEnd - appAreaStart

    // Evenly space app columns
    appColumnXs = []
    for (let i = 0; i < appColumns; i++) {
      appColumnXs.push(appAreaStart + (appAreaWidth / (appColumns + 1)) * (i + 1))
    }
  }

  // Position nodes vertically within their tiers with proper spacing
  const getNodePositions = (nodes: NetworkNode[], tierX: number) => {
    const count = nodes.length
    if (count === 0) return []
    if (count === 1) return [{ node: nodes[0], x: tierX, y: height / 2 }]

    // Calculate actual spacing needed
    const totalHeight = (count - 1) * minSpacing
    const startY = Math.max(padding + nodeHeight / 2, (height - totalHeight) / 2)
    const actualSpacing = count > 1 ? Math.min(minSpacing, (height - padding * 2) / (count - 1)) : 0

    return nodes.map((node, i) => ({
      node,
      x: tierX,
      y: startY + i * actualSpacing,
    }))
  }

  // Position apps across columns - distribute evenly
  const getAppPositions = () => {
    if (appColumns === 1) {
      return getNodePositions(appNodes, appColumnXs[0])
    }

    // Split apps evenly across columns (round-robin)
    const columns: NetworkNode[][] = Array.from({ length: appColumns }, () => [])
    appNodes.forEach((node, i) => {
      columns[i % appColumns].push(node)
    })

    // Get positions for each column
    const positions: { node: NetworkNode; x: number; y: number }[] = []
    columns.forEach((colNodes, colIdx) => {
      const colPositions = getNodePositions(colNodes, appColumnXs[colIdx])
      positions.push(...colPositions)
    })

    return positions
  }

  const externalPos = externalNode ? [{ node: externalNode, x: tier1X, y: height / 2 }] : []
  const appPositions = getAppPositions()
  const systemPositions = getNodePositions(systemNodes, tier3X)
  const allPositions = [...externalPos, ...appPositions, ...systemPositions]

  // Get position by node id
  const getPos = (id: string) => allPositions.find(p => p.node.id === id)

  // Calculate max traffic for scaling line widths
  const maxTraffic = Math.max(...topology.nodes.map(n => n.total_traffic)) || 1

  // Get line width based on traffic
  const getLineWidth = (sourceId: string, targetId: string) => {
    const source = topology.nodes.find(n => n.id === sourceId)
    const target = topology.nodes.find(n => n.id === targetId)
    if (!source || !target) return 1
    const traffic = Math.min(source.total_traffic, target.total_traffic)
    return 1 + (traffic / maxTraffic) * 4
  }

  return (
    <div className="relative w-full h-full">
      {/* SVG for connection lines */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none">
        <defs>
          <linearGradient id="flowGradient" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="rgba(59, 130, 246, 0.6)" />
            <stop offset="100%" stopColor="rgba(34, 197, 94, 0.6)" />
          </linearGradient>
          <marker
            id="arrowMarker"
            markerWidth="8"
            markerHeight="6"
            refX="7"
            refY="3"
            orient="auto"
          >
            <polygon points="0 0, 8 3, 0 6" fill="rgba(107, 114, 128, 0.5)" />
          </marker>
        </defs>

        {/* Draw edges */}
        {topology.edges.map((edge, i) => {
          const sourcePos = getPos(edge.source)
          const targetPos = getPos(edge.target)
          if (!sourcePos || !targetPos) return null

          const edgeConnected = isEdgeConnected(edge.source, edge.target)
          const isHighlighted = hoveredNode === edge.source || hoveredNode === edge.target
          const lineWidth = getLineWidth(edge.source, edge.target)

          // Calculate control points for curved lines
          const midX = (sourcePos.x + targetPos.x) / 2
          const curve = `M ${sourcePos.x + 40} ${sourcePos.y} Q ${midX} ${sourcePos.y} ${midX} ${(sourcePos.y + targetPos.y) / 2} Q ${midX} ${targetPos.y} ${targetPos.x - 40} ${targetPos.y}`

          // Determine stroke color and opacity based on hover - less pronounced effect
          let strokeColor = 'rgba(107, 114, 128, 0.25)'
          let strokeOpacity = 1

          if (hoveredNode) {
            if (edgeConnected) {
              strokeColor = 'rgba(59, 130, 246, 0.5)'
            } else {
              strokeOpacity = 0.4
            }
          }

          if (isHighlighted) {
            strokeColor = 'rgba(59, 130, 246, 0.7)'
          }

          return (
            <g key={`edge-${i}`} style={{ opacity: strokeOpacity }} className="transition-opacity duration-200">
              <path
                d={curve}
                fill="none"
                stroke={strokeColor}
                strokeWidth={isHighlighted ? lineWidth + 1 : lineWidth}
                strokeDasharray={edge.type === 'external' ? '6,4' : undefined}
                markerEnd="url(#arrowMarker)"
                className="transition-all duration-200"
              />
            </g>
          )
        })}
      </svg>

      {/* Tier labels */}
      <div className="absolute top-3 left-0 right-0 flex justify-between text-xs text-gray-400 font-medium uppercase tracking-wide px-8">
        <span style={{ width: 100, textAlign: 'center' }}>External</span>
        <span style={{ width: 100, textAlign: 'center' }}>Applications</span>
        <span style={{ width: 100, textAlign: 'center' }}>System</span>
      </div>

      {/* Render nodes */}
      {allPositions.map(({ node, x, y }) => {
        const isHovered = hoveredNode === node.id
        const nodeConnected = isNodeConnected(node.id)
        const isConnectedToHovered = hoveredNode && !isHovered && nodeConnected

        // Determine visual state - less pronounced fade on hover
        const isFaded = hoveredNode && !nodeConnected

        return (
          <div
            key={node.id}
            className={`absolute transform -translate-x-1/2 -translate-y-1/2 transition-all duration-200 ${
              isHovered ? 'scale-105 z-20' :
              isConnectedToHovered ? 'z-10' :
              'z-0'
            }`}
            style={{
              left: x,
              top: y,
              opacity: isFaded ? 0.5 : 1,
            }}
            onMouseEnter={() => setHoveredNode(node.id)}
            onMouseLeave={() => setHoveredNode(null)}
          >
            <div className={`
              bg-white dark:bg-gray-800 rounded-lg p-2 border-2 transition-all duration-200 w-[90px]
              ${isHovered
                ? 'border-blue-500 shadow-lg shadow-blue-500/20'
                : isConnectedToHovered
                  ? 'border-blue-400/50 shadow-md'
                  : 'border-gray-200 dark:border-gray-700 shadow-sm'
              }
            `}>
              {/* Node icon */}
              <div className="flex justify-center mb-1">
                {node.type === 'external' ? (
                  <div className="w-8 h-8 rounded-lg bg-gray-100 dark:bg-gray-700 flex items-center justify-center">
                    <Globe size={20} className="text-blue-500" />
                  </div>
                ) : (
                  <AppIcon appName={node.id} size={32} />
                )}
              </div>

              {/* Node name */}
              <div className="text-center">
                <div className="text-[10px] font-medium text-gray-900 dark:text-white truncate">
                  {node.name}
                </div>
              </div>

              {/* Traffic indicator - stacked */}
              <div className="mt-1 flex flex-col items-center gap-0 text-[9px]">
                <span className="text-green-500 flex items-center gap-0.5">
                  <ArrowDownToLine size={8} />
                  {formatBandwidth(node.rx_bytes_per_sec).replace(' ', '')}
                </span>
                <span className="text-orange-500 flex items-center gap-0.5">
                  <ArrowUpFromLine size={8} />
                  {formatBandwidth(node.tx_bytes_per_sec).replace(' ', '')}
                </span>
              </div>
            </div>

            {/* Hover tooltip with more details */}
            {isHovered && (
              <div className="absolute left-1/2 -translate-x-1/2 top-full mt-2 bg-gray-900 text-white rounded-lg p-3 text-xs shadow-xl z-30 whitespace-nowrap">
                <div className="font-semibold mb-1">{node.name}</div>
                <div className="space-y-1 text-gray-300">
                  <div className="flex justify-between gap-4">
                    <span>Receive:</span>
                    <span className="text-green-400">{formatBandwidth(node.rx_bytes_per_sec)}</span>
                  </div>
                  <div className="flex justify-between gap-4">
                    <span>Transmit:</span>
                    <span className="text-orange-400">{formatBandwidth(node.tx_bytes_per_sec)}</span>
                  </div>
                  <div className="flex justify-between gap-4">
                    <span>Total:</span>
                    <span className="text-blue-400">{formatBandwidth(node.total_traffic)}</span>
                  </div>
                  {node.pod_count > 0 && (
                    <div className="flex justify-between gap-4">
                      <span>Pods:</span>
                      <span>{node.pod_count}</span>
                    </div>
                  )}
                </div>
                <div className="absolute -top-1 left-1/2 -translate-x-1/2 w-2 h-2 bg-gray-900 rotate-45" />
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}

// ============================================================================
// Stats Table Component
// ============================================================================

function NetworkStatsTable({ stats }: { stats: any[] }) {
  const sortedStats = [...stats].sort((a, b) =>
    (b.rx_bytes_per_sec + b.tx_bytes_per_sec) - (a.rx_bytes_per_sec + a.tx_bytes_per_sec)
  )

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg overflow-hidden border border-gray-200 dark:border-gray-700">
      <table className="w-full">
        <thead>
          <tr className="border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-700/50">
            <th className="text-left px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">App</th>
            <th className="text-right px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">
              <div className="flex items-center justify-end gap-1">
                <ArrowDownToLine size={14} className="text-green-500" />
                Receive
              </div>
            </th>
            <th className="text-right px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">
              <div className="flex items-center justify-end gap-1">
                <ArrowUpFromLine size={14} className="text-orange-500" />
                Transmit
              </div>
            </th>
            <th className="text-right px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">Packets</th>
            <th className="text-right px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">Errors</th>
            <th className="text-right px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">Dropped</th>
            <th className="text-center px-4 py-3 text-sm font-medium text-gray-600 dark:text-gray-300">Status</th>
          </tr>
        </thead>
        <tbody>
          {sortedStats.map(stat => {
            const hasErrors = stat.rx_errors_per_sec > 0 || stat.tx_errors_per_sec > 0
            const hasDropped = stat.rx_dropped_per_sec > 0 || stat.tx_dropped_per_sec > 0

            return (
              <tr
                key={stat.namespace}
                className="border-b border-gray-200 dark:border-gray-700/50 hover:bg-gray-50 dark:hover:bg-gray-700/30"
              >
                <td className="px-4 py-3">
                  <div className="flex items-center gap-3">
                    <AppIcon appName={stat.namespace} size={32} />
                    <span className="font-medium text-gray-900 dark:text-white">{stat.app_name}</span>
                  </div>
                </td>
                <td className="text-right px-4 py-3">
                  <span className="text-green-500 font-mono text-sm">
                    {formatBandwidth(stat.rx_bytes_per_sec)}
                  </span>
                </td>
                <td className="text-right px-4 py-3">
                  <span className="text-orange-500 font-mono text-sm">
                    {formatBandwidth(stat.tx_bytes_per_sec)}
                  </span>
                </td>
                <td className="text-right px-4 py-3">
                  <div className="text-gray-500 dark:text-gray-400 font-mono text-xs">
                    <div>{formatPackets(stat.rx_packets_per_sec)} in</div>
                    <div>{formatPackets(stat.tx_packets_per_sec)} out</div>
                  </div>
                </td>
                <td className="text-right px-4 py-3">
                  <span className={`font-mono text-xs ${hasErrors ? 'text-red-500' : 'text-gray-400'}`}>
                    {stat.rx_errors_per_sec + stat.tx_errors_per_sec > 0
                      ? (stat.rx_errors_per_sec + stat.tx_errors_per_sec).toFixed(2)
                      : '0'}
                  </span>
                </td>
                <td className="text-right px-4 py-3">
                  <span className={`font-mono text-xs ${hasDropped ? 'text-yellow-500' : 'text-gray-400'}`}>
                    {stat.rx_dropped_per_sec + stat.tx_dropped_per_sec > 0
                      ? (stat.rx_dropped_per_sec + stat.tx_dropped_per_sec).toFixed(2)
                      : '0'}
                  </span>
                </td>
                <td className="text-center px-4 py-3">
                  {hasErrors ? (
                    <span className="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs bg-red-100 dark:bg-red-900/30 text-red-600 dark:text-red-400">
                      <WifiOff size={12} />
                      Errors
                    </span>
                  ) : hasDropped ? (
                    <span className="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs bg-yellow-100 dark:bg-yellow-900/30 text-yellow-600 dark:text-yellow-400">
                      <AlertCircle size={12} />
                      Drops
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">
                      <Wifi size={12} />
                      Healthy
                    </span>
                  )}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

// ============================================================================
// Main Page Component
// ============================================================================

export default function NetworkingPage() {
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [graphSize, setGraphSize] = useState({ width: 800, height: 400 })
  const graphContainerRef = useRef<HTMLDivElement>(null)

  // Resize observer for graph container
  useEffect(() => {
    if (!graphContainerRef.current) return

    const observer = new ResizeObserver(entries => {
      for (const entry of entries) {
        setGraphSize({
          width: entry.contentRect.width,
          height: Math.max(350, entry.contentRect.height),
        })
      }
    })

    observer.observe(graphContainerRef.current)
    return () => observer.disconnect()
  }, [])

  // Fetch topology
  const {
    data: topology,
    isLoading: topologyLoading,
    refetch: refetchTopology,
    error: topologyError,
  } = useQuery({
    queryKey: ['networking', 'topology'],
    queryFn: networkingApi.getTopology,
    refetchInterval: autoRefresh ? 15000 : false,
  })

  // Fetch stats
  const {
    data: stats,
    isLoading: statsLoading,
    refetch: refetchStats,
  } = useQuery({
    queryKey: ['networking', 'stats'],
    queryFn: networkingApi.getStats,
    refetchInterval: autoRefresh ? 15000 : false,
  })

  const handleRefresh = () => {
    refetchTopology()
    refetchStats()
  }

  // Calculate totals
  const totalRx = stats?.reduce((sum, s) => sum + s.rx_bytes_per_sec, 0) || 0
  const totalTx = stats?.reduce((sum, s) => sum + s.tx_bytes_per_sec, 0) || 0
  const totalErrors = stats?.reduce((sum, s) => sum + s.rx_errors_per_sec + s.tx_errors_per_sec, 0) || 0
  const totalDropped = stats?.reduce((sum, s) => sum + s.rx_dropped_per_sec + s.tx_dropped_per_sec, 0) || 0

  // Get internet traffic from the external node
  const externalNode = topology?.nodes.find(n => n.type === 'external')
  const internetTraffic = externalNode ? externalNode.rx_bytes_per_sec + externalNode.tx_bytes_per_sec : totalRx + totalTx

  if (topologyError) {
    return (
      <div className="flex flex-col items-center justify-center h-[60vh] text-center">
        <AlertCircle size={64} className="text-red-500 mb-4" />
        <h2 className="text-2xl font-bold mb-2 text-gray-900 dark:text-white">Error Loading Network Data</h2>
        <p className="text-gray-500 dark:text-gray-400 max-w-md">
          Could not fetch network topology data. Make sure VictoriaMetrics is running.
        </p>
        <button
          onClick={handleRefresh}
          className="mt-4 px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg text-white"
        >
          Retry
        </button>
      </div>
    )
  }

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2 text-gray-900 dark:text-white">
            <Network className="text-blue-500 dark:text-blue-400" />
            Networking
          </h1>
          <p className="text-gray-500 dark:text-gray-400 mt-1">
            Network topology and traffic flow between applications
          </p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg transition-colors ${
              autoRefresh
                ? 'bg-green-600 hover:bg-green-700 text-white'
                : 'bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 text-gray-700 dark:text-white'
            }`}
          >
            <Gauge size={18} />
            {autoRefresh ? 'Live' : 'Paused'}
          </button>
          <button
            onClick={handleRefresh}
            disabled={topologyLoading || statsLoading}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors disabled:opacity-50 text-white"
          >
            <RefreshCw size={18} className={topologyLoading || statsLoading ? 'animate-spin' : ''} />
            Refresh
          </button>
        </div>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
        <div className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 mb-2">
            <Globe size={18} className="text-blue-500" />
            <span className="text-sm">Internet Traffic</span>
          </div>
          <div className="text-2xl font-bold text-blue-500">{formatBandwidth(internetTraffic)}</div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 mb-2">
            <ArrowDownToLine size={18} className="text-green-500" />
            <span className="text-sm">Total Receive</span>
          </div>
          <div className="text-2xl font-bold text-green-500">{formatBandwidth(totalRx)}</div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 mb-2">
            <ArrowUpFromLine size={18} className="text-orange-500" />
            <span className="text-sm">Total Transmit</span>
          </div>
          <div className="text-2xl font-bold text-orange-500">{formatBandwidth(totalTx)}</div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 mb-2">
            <Zap size={18} className={totalErrors > 0 ? 'text-red-500' : 'text-gray-400'} />
            <span className="text-sm">Errors/sec</span>
          </div>
          <div className={`text-2xl font-bold ${totalErrors > 0 ? 'text-red-500' : 'text-gray-400'}`}>
            {totalErrors.toFixed(2)}
          </div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400 mb-2">
            <AlertCircle size={18} className={totalDropped > 0 ? 'text-yellow-500' : 'text-gray-400'} />
            <span className="text-sm">Dropped/sec</span>
          </div>
          <div className={`text-2xl font-bold ${totalDropped > 0 ? 'text-yellow-500' : 'text-gray-400'}`}>
            {totalDropped.toFixed(2)}
          </div>
        </div>
      </div>

      {/* Network Flow Visualization */}
      <div>
        <h2 className="text-xl font-semibold mb-4 flex items-center gap-2 text-gray-900 dark:text-white">
          <Globe size={20} />
          Network Flow
          <span className="text-sm font-normal text-gray-500 dark:text-gray-400 ml-2">
            Hover over an app to highlight its connections
          </span>
        </h2>
        <div
          ref={graphContainerRef}
          className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden"
          style={{ height: '450px' }}
        >
          {topologyLoading ? (
            <div className="flex items-center justify-center h-full">
              <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
            </div>
          ) : topology ? (
            <NetworkFlowVisualization
              topology={topology}
              width={graphSize.width}
              height={graphSize.height}
            />
          ) : null}
        </div>
      </div>

      {/* Network Statistics Table */}
      <div>
        <h2 className="text-xl font-semibold mb-4 flex items-center gap-2 text-gray-900 dark:text-white">
          <Network size={20} />
          Network Statistics
        </h2>
        {statsLoading ? (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
          </div>
        ) : stats && stats.length > 0 ? (
          <NetworkStatsTable stats={stats} />
        ) : (
          <div className="bg-white dark:bg-gray-800 rounded-lg p-8 text-center text-gray-500 dark:text-gray-400 border border-gray-200 dark:border-gray-700">
            <Network size={48} className="mx-auto mb-4 opacity-50" />
            <p>No network statistics available</p>
            <p className="text-sm mt-1">Install some apps to see their network usage</p>
          </div>
        )}
      </div>

      {/* Auto-refresh indicator */}
      {autoRefresh && (
        <div className="text-center text-sm text-gray-500 dark:text-gray-500">
          Auto-refreshing every 15 seconds
        </div>
      )}
    </div>
  )
}
