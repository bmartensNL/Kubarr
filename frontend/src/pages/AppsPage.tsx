import { useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { appsApi } from '../api/apps'

type AppState = 'idle' | 'installing' | 'installed' | 'deleting' | 'error'

interface AppStatus {
  state: AppState
  message?: string
}

export default function AppsPage() {
  const queryClient = useQueryClient()
  const [toast, setToast] = useState<{ message: string; type: 'success' | 'error' } | null>(null)
  const [appStatuses, setAppStatuses] = useState<Record<string, AppStatus>>({})

  const { data: catalog, isLoading } = useQuery({
    queryKey: ['apps', 'catalog'],
    queryFn: appsApi.getCatalog,
  })

  const { data: installed } = useQuery({
    queryKey: ['apps', 'installed'],
    queryFn: () => appsApi.getInstalled(),
    refetchInterval: 3000, // Refresh installed apps every 3 seconds
  })

  // Fetch status for each app when catalog loads
  useEffect(() => {
    if (catalog && catalog.length > 0) {
      catalog.forEach(async (app) => {
        try {
          const status = await appsApi.getStatus(app.name)
          updateAppState(app.name, status.state as AppState, status.message)
        } catch (error) {
          // Silently handle - app might not exist yet
        }
      })
    }
  }, [catalog])

  const showToast = (message: string, type: 'success' | 'error') => {
    setToast({ message, type })
    setTimeout(() => setToast(null), 5000)
  }

  const updateAppState = (appName: string, state: AppState, message?: string) => {
    setAppStatuses(prev => ({
      ...prev,
      [appName]: { state, message }
    }))
  }

  // Poll for health after installation
  const pollHealth = async (appName: string) => {
    const maxAttempts = 60 // 60 attempts * 2 seconds = 2 minutes
    let attempts = 0

    const checkHealth = async (): Promise<boolean> => {
      try {
        const health = await appsApi.checkHealth(appName)

        if (health.healthy && health.status === 'healthy') {
          updateAppState(appName, 'installed')
          queryClient.invalidateQueries({ queryKey: ['apps', 'installed'] })
          showToast(`${appName} installed successfully`, 'success')
          return true
        }

        attempts++
        if (attempts >= maxAttempts) {
          updateAppState(appName, 'error', 'Installation timeout - deployments not healthy')
          showToast(`${appName} installation timed out`, 'error')
          return true
        }

        // Continue polling
        setTimeout(() => checkHealth(), 2000)
        return false
      } catch (error) {
        attempts++
        if (attempts >= maxAttempts) {
          updateAppState(appName, 'error', 'Health check failed')
          showToast(`${appName} health check failed`, 'error')
          return true
        }
        setTimeout(() => checkHealth(), 2000)
        return false
      }
    }

    checkHealth()
  }

  // Poll for namespace deletion
  const pollDeletion = async (appName: string) => {
    const maxAttempts = 60 // 60 attempts * 2 seconds = 2 minutes
    let attempts = 0

    const checkDeletion = async (): Promise<boolean> => {
      try {
        const { exists } = await appsApi.checkExists(appName)

        if (!exists) {
          updateAppState(appName, 'idle')
          queryClient.invalidateQueries({ queryKey: ['apps', 'installed'] })
          showToast(`${appName} uninstalled successfully`, 'success')
          return true
        }

        attempts++
        if (attempts >= maxAttempts) {
          updateAppState(appName, 'error', 'Deletion timeout')
          showToast(`${appName} deletion timed out`, 'error')
          return true
        }

        // Continue polling
        setTimeout(() => checkDeletion(), 2000)
        return false
      } catch (error) {
        attempts++
        if (attempts >= maxAttempts) {
          updateAppState(appName, 'error', 'Deletion check failed')
          showToast(`${appName} deletion check failed`, 'error')
          return true
        }
        setTimeout(() => checkDeletion(), 2000)
        return false
      }
    }

    checkDeletion()
  }

  const installMutation = useMutation({
    mutationFn: (appName: string) => {
      updateAppState(appName, 'installing')
      return appsApi.install({ app_name: appName, namespace: appName })
    },
    onSuccess: (_data, appName) => {
      // Start polling for health
      pollHealth(appName)
    },
    onError: (error: any, appName) => {
      updateAppState(appName, 'error', error.response?.data?.detail || error.message)
      showToast(`Failed to install ${appName}: ${error.response?.data?.detail || error.message}`, 'error')
    },
  })

  const deleteMutation = useMutation({
    mutationFn: (appName: string) => {
      updateAppState(appName, 'deleting')
      return appsApi.delete(appName)
    },
    onSuccess: (_data, appName) => {
      // Start polling for namespace deletion
      pollDeletion(appName)
    },
    onError: (error: any, appName) => {
      updateAppState(appName, 'error', error.response?.data?.detail || error.message)
      showToast(`Failed to uninstall ${appName}: ${error.response?.data?.detail || error.message}`, 'error')
    },
  })

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
      </div>
    )
  }

  return (
    <div className="space-y-8">
      {/* Toast Notification */}
      {toast && (
        <div className={`fixed top-4 right-4 z-50 px-6 py-4 rounded-lg shadow-lg border ${
          toast.type === 'success'
            ? 'bg-green-900 border-green-700 text-green-100'
            : 'bg-red-900 border-red-700 text-red-100'
        }`}>
          <div className="flex items-center gap-3">
            <div className="flex-shrink-0">
              {toast.type === 'success' ? (
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
                  <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                </svg>
              ) : (
                <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
                  <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                </svg>
              )}
            </div>
            <div className="flex-1">{toast.message}</div>
            <button
              onClick={() => setToast(null)}
              className="flex-shrink-0 hover:opacity-75"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clipRule="evenodd" />
              </svg>
            </button>
          </div>
        </div>
      )}

      <div>
        <h2 className="text-2xl font-bold mb-2">App Catalog</h2>
        <p className="text-gray-400">Browse and manage applications</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {catalog?.map((app) => {
          const isInstalled = installed?.includes(app.name)
          const appStatus = appStatuses[app.name] || { state: isInstalled ? 'installed' : 'idle' }
          const isOperating = appStatus.state === 'installing' || appStatus.state === 'deleting'

          return (
            <div
              key={app.name}
              className={`bg-gray-800 rounded-lg p-6 flex flex-col relative ${
                isOperating ? 'opacity-75' : ''
              }`}
            >
              {/* Loading Overlay */}
              {isOperating && (
                <div className="absolute inset-0 bg-gray-900 bg-opacity-50 rounded-lg flex items-center justify-center z-10">
                  <div className="flex flex-col items-center gap-3">
                    <div className="animate-spin rounded-full h-10 w-10 border-b-2 border-blue-500"></div>
                    <span className="text-sm text-gray-300 font-medium">
                      {appStatus.state === 'installing' ? 'Installing...' : 'Uninstalling...'}
                    </span>
                  </div>
                </div>
              )}

              <div className="flex items-start justify-between mb-4">
                <div className="flex-1">
                  <h3 className="text-xl font-semibold mb-1">{app.display_name}</h3>
                  <p className="text-sm text-gray-400 capitalize">{app.category}</p>
                </div>
                {appStatus.state === 'installed' && (
                  <span className="bg-green-600 text-white text-xs px-2 py-1 rounded">
                    Installed
                  </span>
                )}
                {appStatus.state === 'installing' && (
                  <span className="bg-blue-600 text-white text-xs px-2 py-1 rounded animate-pulse">
                    Installing
                  </span>
                )}
                {appStatus.state === 'deleting' && (
                  <span className="bg-red-600 text-white text-xs px-2 py-1 rounded animate-pulse">
                    Deleting
                  </span>
                )}
                {appStatus.state === 'error' && (
                  <span className="bg-red-900 text-white text-xs px-2 py-1 rounded">
                    Error
                  </span>
                )}
              </div>

              <p className="text-sm text-gray-300 mb-4 flex-1">{app.description}</p>

              <div className="flex gap-2">
                {appStatus.state === 'installed' ? (
                  <>
                    <button
                      onClick={() => deleteMutation.mutate(app.name)}
                      disabled={isOperating}
                      className="flex-1 bg-red-600 hover:bg-red-700 disabled:bg-red-800 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded transition-colors"
                    >
                      Delete
                    </button>
                    <a
                      href={`/${app.name}/`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex-1 bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded transition-colors text-center"
                    >
                      View
                    </a>
                  </>
                ) : appStatus.state === 'idle' || appStatus.state === 'error' ? (
                  <button
                    onClick={() => installMutation.mutate(app.name)}
                    disabled={isOperating}
                    className="w-full bg-blue-600 hover:bg-blue-700 disabled:bg-blue-800 disabled:cursor-not-allowed text-white font-medium py-2 px-4 rounded transition-colors"
                  >
                    {appStatus.state === 'error' ? 'Retry Install' : 'Install'}
                  </button>
                ) : (
                  <button
                    disabled
                    className="w-full bg-gray-700 cursor-not-allowed text-white font-medium py-2 px-4 rounded"
                  >
                    {appStatus.state === 'installing' ? 'Installing...' : 'Deleting...'}
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
