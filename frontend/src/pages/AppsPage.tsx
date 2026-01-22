import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { appsApi } from '../api/apps'

export default function AppsPage() {
  const queryClient = useQueryClient()

  const { data: catalog, isLoading } = useQuery({
    queryKey: ['apps', 'catalog'],
    queryFn: appsApi.getCatalog,
  })

  const { data: installed } = useQuery({
    queryKey: ['apps', 'installed'],
    queryFn: () => appsApi.getInstalled(),
  })

  const installMutation = useMutation({
    mutationFn: (appName: string) =>
      appsApi.install({ app_name: appName, namespace: 'media' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apps', 'installed'] })
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (appName: string) => appsApi.delete(appName, 'media'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apps', 'installed'] })
    },
  })

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-gray-400">Loading apps...</div>
      </div>
    )
  }

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-2xl font-bold mb-2">App Catalog</h2>
        <p className="text-gray-400">Browse and manage applications</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {catalog?.map((app) => {
          const isInstalled = installed?.includes(app.name)

          return (
            <div key={app.name} className="bg-gray-800 rounded-lg p-6 flex flex-col">
              <div className="flex items-start justify-between mb-4">
                <div className="flex-1">
                  <h3 className="text-xl font-semibold mb-1">{app.display_name}</h3>
                  <p className="text-sm text-gray-400 capitalize">{app.category}</p>
                </div>
                {isInstalled && (
                  <span className="bg-green-600 text-white text-xs px-2 py-1 rounded">
                    Installed
                  </span>
                )}
              </div>

              <p className="text-sm text-gray-300 mb-4 flex-1">{app.description}</p>

              <div className="space-y-2 text-sm text-gray-400 mb-4">
                <div className="flex justify-between">
                  <span>Port:</span>
                  <span>{app.default_port}</span>
                </div>
                <div className="flex justify-between">
                  <span>CPU:</span>
                  <span>{app.resource_requirements.cpu_limit}</span>
                </div>
                <div className="flex justify-between">
                  <span>Memory:</span>
                  <span>{app.resource_requirements.memory_limit}</span>
                </div>
              </div>

              <div className="flex gap-2">
                {isInstalled ? (
                  <>
                    <button
                      onClick={() => deleteMutation.mutate(app.name)}
                      disabled={deleteMutation.isPending}
                      className="flex-1 bg-red-600 hover:bg-red-700 disabled:bg-red-800 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded"
                    >
                      {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
                    </button>
                    <button className="flex-1 bg-gray-700 hover:bg-gray-600 text-white font-medium py-2 px-4 rounded">
                      View
                    </button>
                  </>
                ) : (
                  <button
                    onClick={() => installMutation.mutate(app.name)}
                    disabled={installMutation.isPending}
                    className="w-full bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded"
                  >
                    {installMutation.isPending ? 'Installing...' : 'Install'}
                  </button>
                )}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
