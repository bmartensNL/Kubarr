import { useQuery } from '@tanstack/react-query'
import { appsApi } from '../api/apps'
import { monitoringApi } from '../api/monitoring'

export default function Dashboard() {
  const { data: catalog, isLoading: catalogLoading } = useQuery({
    queryKey: ['apps', 'catalog'],
    queryFn: appsApi.getCatalog,
  })

  const { data: installed, isLoading: installedLoading } = useQuery({
    queryKey: ['apps', 'installed'],
    queryFn: () => appsApi.getInstalled(),
  })

  const { data: podStatus } = useQuery({
    queryKey: ['monitoring', 'pods'],
    queryFn: () => monitoringApi.getPodStatus(),
    refetchInterval: 5000, // Refresh every 5 seconds
  })

  if (catalogLoading || installedLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-gray-400">Loading...</div>
      </div>
    )
  }

  const installedApps = catalog?.filter((app) => installed?.includes(app.name)) || []
  const availableApps = catalog?.filter((app) => !installed?.includes(app.name)) || []

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-2xl font-bold mb-4">Overview</h2>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Total Apps</div>
            <div className="text-3xl font-bold">{catalog?.length || 0}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Installed</div>
            <div className="text-3xl font-bold text-green-400">{installed?.length || 0}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Running Pods</div>
            <div className="text-3xl font-bold text-blue-400">
              {podStatus?.filter((p) => p.status === 'Running').length || 0}
            </div>
          </div>
        </div>
      </div>

      {installedApps.length > 0 && (
        <div>
          <h2 className="text-2xl font-bold mb-4">Installed Apps</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {installedApps.map((app) => {
              const appPods = podStatus?.filter((p) => p.app === app.name) || []
              const runningPods = appPods.filter((p) => p.status === 'Running').length
              const isHealthy = appPods.length > 0 && runningPods === appPods.length

              return (
                <div key={app.name} className="bg-gray-800 rounded-lg p-6">
                  <div className="flex items-start justify-between mb-4">
                    <div>
                      <h3 className="text-lg font-semibold">{app.display_name}</h3>
                      <p className="text-sm text-gray-400">{app.category}</p>
                    </div>
                    <div
                      className={`w-3 h-3 rounded-full ${
                        isHealthy ? 'bg-green-400' : 'bg-red-400'
                      }`}
                    />
                  </div>
                  <p className="text-sm text-gray-300 mb-4">{app.description}</p>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-gray-400">
                      {runningPods}/{appPods.length} pods running
                    </span>
                    <span className="text-gray-400">Port {app.default_port}</span>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {availableApps.length > 0 && (
        <div>
          <h2 className="text-2xl font-bold mb-4">Available Apps</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {availableApps.slice(0, 6).map((app) => (
              <div key={app.name} className="bg-gray-800 rounded-lg p-6">
                <div className="mb-4">
                  <h3 className="text-lg font-semibold">{app.display_name}</h3>
                  <p className="text-sm text-gray-400">{app.category}</p>
                </div>
                <p className="text-sm text-gray-300 mb-4">{app.description}</p>
                <button className="w-full bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-4 rounded">
                  Install
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
