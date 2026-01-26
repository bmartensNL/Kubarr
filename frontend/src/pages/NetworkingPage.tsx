import { useState, useMemo, useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { networkingApi, NetworkTopology, formatBandwidth, formatPackets } from '../api/networking'
import { AppIcon } from '../components/AppIcon'
import {
  ReactFlow,
  Node,
  Edge,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  Position,
  Handle,
  BaseEdge,
  EdgeProps,
  getBezierPath,
  ReactFlowProvider,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import dagre from 'dagre'
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
// Custom Node Component
// ============================================================================

interface TrafficNodeData {
  label: string
  type: 'external' | 'app'
  rx: number
  tx: number
  total: number
  podCount: number
  appId: string
  [key: string]: unknown
}

function TrafficNode({ data }: { data: TrafficNodeData }) {
  return (
    <div className="relative">
      {/* Handles for connections */}
      <Handle
        type="target"
        position={Position.Top}
        className="!bg-blue-500 !w-2 !h-2 !border-2 !border-white dark:!border-gray-800"
      />
      <Handle
        type="source"
        position={Position.Bottom}
        className="!bg-blue-500 !w-2 !h-2 !border-2 !border-white dark:!border-gray-800"
      />

      {/* Node Card */}
      <div className="bg-white dark:bg-gray-800 rounded-xl p-3 border-2 border-gray-200 dark:border-gray-600 shadow-lg hover:shadow-xl hover:border-blue-500 transition-all duration-200 min-w-[100px]">
        {/* Icon */}
        <div className="flex justify-center mb-2">
          {data.type === 'external' ? (
            <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-blue-500 to-blue-600 flex items-center justify-center shadow-md">
              <Globe size={24} className="text-white" />
            </div>
          ) : (
            <AppIcon appName={data.appId} size={40} className="rounded-xl shadow-md" />
          )}
        </div>

        {/* Name */}
        <div className="text-center mb-2">
          <div className="text-xs font-semibold text-gray-900 dark:text-white truncate max-w-[90px]">
            {data.label}
          </div>
        </div>

        {/* Traffic Stats */}
        <div className="flex flex-col gap-0.5 text-[10px]">
          <div className="flex items-center justify-center gap-1 text-green-500">
            <ArrowDownToLine size={10} />
            <span className="font-mono">{formatBandwidth(data.rx)}</span>
          </div>
          <div className="flex items-center justify-center gap-1 text-orange-500">
            <ArrowUpFromLine size={10} />
            <span className="font-mono">{formatBandwidth(data.tx)}</span>
          </div>
        </div>

        {/* Pod count badge */}
        {data.podCount > 0 && (
          <div className="absolute -top-2 -right-2 bg-blue-500 text-white text-[9px] font-bold rounded-full w-5 h-5 flex items-center justify-center shadow">
            {data.podCount}
          </div>
        )}
      </div>
    </div>
  )
}

// ============================================================================
// Animated Edge Component
// ============================================================================

interface AnimatedEdgeData {
  traffic: number
  maxTraffic: number
  [key: string]: unknown
}

function AnimatedEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  style,
}: EdgeProps<Edge<AnimatedEdgeData>>) {
  const [edgePath] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  })

  // Calculate line width based on traffic (2-8px)
  const traffic = data?.traffic || 0
  const maxTraffic = data?.maxTraffic || 1
  const strokeWidth = 2 + (traffic / maxTraffic) * 6

  // Animation speed based on traffic (faster = more traffic)
  const animationDuration = Math.max(1, 4 - (traffic / maxTraffic) * 3)

  return (
    <>
      {/* Background path */}
      <BaseEdge
        id={id}
        path={edgePath}
        style={{
          ...style,
          strokeWidth,
          stroke: 'rgba(59, 130, 246, 0.2)',
        }}
      />

      {/* Animated particles */}
      <circle r="4" fill="#3b82f6">
        <animateMotion
          dur={`${animationDuration}s`}
          repeatCount="indefinite"
          path={edgePath}
        />
      </circle>
      <circle r="4" fill="#3b82f6" style={{ opacity: 0.6 }}>
        <animateMotion
          dur={`${animationDuration}s`}
          repeatCount="indefinite"
          path={edgePath}
          begin={`${animationDuration / 3}s`}
        />
      </circle>
      <circle r="4" fill="#3b82f6" style={{ opacity: 0.3 }}>
        <animateMotion
          dur={`${animationDuration}s`}
          repeatCount="indefinite"
          path={edgePath}
          begin={`${(animationDuration / 3) * 2}s`}
        />
      </circle>
    </>
  )
}

// ============================================================================
// Dagre Layout
// ============================================================================

const nodeWidth = 120
const nodeHeight = 100

function getLayoutedElements(
  nodes: Node[],
  edges: Edge[],
  direction: 'TB' | 'LR' = 'TB'
) {
  const dagreGraph = new dagre.graphlib.Graph()
  dagreGraph.setDefaultEdgeLabel(() => ({}))

  const isHorizontal = direction === 'LR'
  dagreGraph.setGraph({ rankdir: direction, nodesep: 80, ranksep: 100 })

  nodes.forEach((node) => {
    dagreGraph.setNode(node.id, { width: nodeWidth, height: nodeHeight })
  })

  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target)
  })

  dagre.layout(dagreGraph)

  const layoutedNodes = nodes.map((node) => {
    const nodeWithPosition = dagreGraph.node(node.id)
    return {
      ...node,
      position: {
        x: nodeWithPosition.x - nodeWidth / 2,
        y: nodeWithPosition.y - nodeHeight / 2,
      },
      targetPosition: isHorizontal ? Position.Left : Position.Top,
      sourcePosition: isHorizontal ? Position.Right : Position.Bottom,
    }
  })

  return { nodes: layoutedNodes, edges }
}

// ============================================================================
// Network Flow Visualization with React Flow
// ============================================================================

const nodeTypes = { traffic: TrafficNode }
const edgeTypes = { animated: AnimatedEdge }

interface FlowVisualizationProps {
  topology: NetworkTopology
}

function NetworkFlowVisualizationInner({ topology }: FlowVisualizationProps) {
  // Convert topology to React Flow nodes and edges
  const { initialNodes, initialEdges } = useMemo(() => {
    const maxTraffic = Math.max(...topology.nodes.map(n => n.total_traffic), 1)

    const nodes: Node<TrafficNodeData>[] = topology.nodes.map((node) => ({
      id: node.id,
      type: 'traffic',
      position: { x: 0, y: 0 }, // Will be set by dagre
      data: {
        label: node.name,
        type: node.type,
        rx: node.rx_bytes_per_sec,
        tx: node.tx_bytes_per_sec,
        total: node.total_traffic,
        podCount: node.pod_count,
        appId: node.id,
      },
    }))

    const edges: Edge<AnimatedEdgeData>[] = topology.edges.map((edge, i) => {
      const sourceNode = topology.nodes.find(n => n.id === edge.source)
      const targetNode = topology.nodes.find(n => n.id === edge.target)
      const traffic = Math.min(sourceNode?.total_traffic || 0, targetNode?.total_traffic || 0)

      return {
        id: `e${i}-${edge.source}-${edge.target}`,
        source: edge.source,
        target: edge.target,
        type: 'animated',
        data: { traffic, maxTraffic },
      }
    })

    return { initialNodes: nodes, initialEdges: edges, maxTraffic }
  }, [topology])

  // Apply dagre layout
  const { nodes: layoutedNodes, edges: layoutedEdges } = useMemo(
    () => getLayoutedElements(initialNodes, initialEdges, 'TB'),
    [initialNodes, initialEdges]
  )

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutedNodes)
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutedEdges)

  // Update nodes when topology changes
  useEffect(() => {
    const { nodes: newLayoutedNodes, edges: newLayoutedEdges } = getLayoutedElements(
      initialNodes,
      initialEdges,
      'TB'
    )
    setNodes(newLayoutedNodes)
    setEdges(newLayoutedEdges)
  }, [initialNodes, initialEdges, setNodes, setEdges])

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

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      minZoom={0.5}
      maxZoom={1.5}
      proOptions={{ hideAttribution: true }}
      className="bg-gray-50 dark:bg-gray-900"
    >
      <Background color="#e5e7eb" gap={20} className="dark:!bg-gray-900" />
      <Controls className="!bg-white dark:!bg-gray-800 !border-gray-200 dark:!border-gray-700 !shadow-lg [&>button]:!bg-white [&>button]:dark:!bg-gray-800 [&>button]:!border-gray-200 [&>button]:dark:!border-gray-700 [&>button]:!text-gray-600 [&>button]:dark:!text-gray-300 [&>button:hover]:!bg-gray-100 [&>button:hover]:dark:!bg-gray-700" />
      <MiniMap
        nodeColor={(node) => {
          const data = node.data as TrafficNodeData
          if (data.type === 'external') return '#3b82f6'
          return '#10b981'
        }}
        className="!bg-white dark:!bg-gray-800 !border-gray-200 dark:!border-gray-700"
        maskColor="rgba(0, 0, 0, 0.1)"
      />
    </ReactFlow>
  )
}

function NetworkFlowVisualization({ topology }: FlowVisualizationProps) {
  return (
    <ReactFlowProvider>
      <NetworkFlowVisualizationInner topology={topology} />
    </ReactFlowProvider>
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
            Drag to pan • Scroll to zoom • Use controls for navigation
          </span>
        </h2>
        <div
          className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden"
          style={{ height: '500px' }}
        >
          {topologyLoading ? (
            <div className="flex items-center justify-center h-full">
              <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
            </div>
          ) : topology ? (
            <NetworkFlowVisualization topology={topology} />
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
