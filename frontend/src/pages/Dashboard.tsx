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
      <div className="flex items-center justify-center h-[60vh]">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
      </div>
    )
  }

  const healthyApps = installedApps.filter((app) => {
    const appPods = podStatusByApp[app.name] || []
    return appPods.length > 0 && appPods.every((p) => p.ready && p.status === 'Running')
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

      {/* App Grid - Launchpad Style */}
      {installedApps.length > 0 ? (
        <div>
          <h2 className="text-2xl font-bold mb-4">Installed Apps</h2>
          <div className="grid grid-cols-4 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-6">
            {installedApps.map((app) => {
              const appPods = podStatusByApp[app.name] || []
              const isHealthy = appPods.length > 0 && appPods.every((p) => p.ready && p.status === 'Running')
              const isLoading = podQueries[installedApps.indexOf(app)]?.isLoading

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

                    {/* Health Indicator */}
                    {!isLoading && (
                      <div
                        className={`absolute -bottom-1 -right-1 w-4 h-4 rounded-full border-2 border-gray-900 ${
                          isHealthy ? 'bg-green-500' : 'bg-red-500'
                        }`}
                        title={isHealthy ? 'Running' : 'Not Ready'}
                      />
                    )}

                    {/* Loading indicator */}
                    {isLoading && (
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
