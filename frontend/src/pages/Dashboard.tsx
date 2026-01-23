import { useQuery, useQueries } from '@tanstack/react-query'
import { appsApi } from '../api/apps'
import { monitoringApi } from '../api/monitoring'
import { AppIcon } from '../components/AppIcon'
import type { PodStatus } from '../types'

export default function Dashboard() {
  const { data: catalog, isLoading: catalogLoading } = useQuery({
    queryKey: ['apps', 'catalog'],
    queryFn: appsApi.getCatalog,
  })

  const { data: installed, isLoading: installedLoading } = useQuery({
    queryKey: ['apps', 'installed'],
    queryFn: () => appsApi.getInstalled(),
  })

  const installedApps = catalog?.filter((app) => installed?.includes(app.name)) || []

  // Query pod status for each installed app's namespace
  const podQueries = useQueries({
    queries: installedApps.map((app) => ({
      queryKey: ['monitoring', 'pods', app.name],
      queryFn: () => monitoringApi.getPodStatus(app.name),
      refetchInterval: 5000,
    })),
  })

  // Build a map of app name to pod status
  const podStatusByApp: Record<string, PodStatus[]> = {}
  installedApps.forEach((app, index) => {
    podStatusByApp[app.name] = podQueries[index]?.data || []
  })

  if (catalogLoading || installedLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-gray-400">Loading...</div>
      </div>
    )
  }

  const healthyApps = installedApps.filter((app) => {
    const appPods = podStatusByApp[app.name] || []
    return appPods.length > 0 && appPods.every((p) => p.ready && p.status === 'Running')
  })

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-2xl font-bold mb-4">Overview</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Installed Apps</div>
            <div className="text-3xl font-bold">{installedApps.length}</div>
          </div>
          <div className="bg-gray-800 rounded-lg p-6">
            <div className="text-gray-400 text-sm">Healthy</div>
            <div className="text-3xl font-bold text-green-400">{healthyApps.length}</div>
          </div>
        </div>
      </div>

      {installedApps.length > 0 && (
        <div>
          <h2 className="text-2xl font-bold mb-4">Installed Apps</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {installedApps.map((app) => {
              const appPods = podStatusByApp[app.name] || []
              const isHealthy = appPods.length > 0 && appPods.every((p) => p.ready && p.status === 'Running')

              return (
                <div key={app.name} className="bg-gray-800 rounded-lg p-6">
                  <div className="flex items-start gap-4 mb-4">
                    <AppIcon appName={app.name} size={48} />
                    <div className="flex-1">
                      <div className="flex items-center justify-between">
                        <h3 className="text-lg font-semibold">{app.display_name}</h3>
                        <div
                          className={`w-3 h-3 rounded-full ${
                            isHealthy ? 'bg-green-400' : 'bg-red-400'
                          }`}
                          title={isHealthy ? 'Healthy' : 'Unhealthy'}
                        />
                      </div>
                      <p className="text-sm text-gray-400">{app.category}</p>
                    </div>
                  </div>
                  <p className="text-sm text-gray-300 mb-4">{app.description}</p>
                  <a
                    href={`/${app.name}/`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-block w-full text-center bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-4 rounded"
                  >
                    View
                  </a>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {installedApps.length === 0 && (
        <div className="text-center py-12">
          <p className="text-gray-400">No apps installed yet. Go to the Apps page to install some.</p>
        </div>
      )}
    </div>
  )
}
