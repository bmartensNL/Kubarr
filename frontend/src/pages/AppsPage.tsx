import { useState, useEffect, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { appsApi } from '../api/apps'
import { AppIcon } from '../components/AppIcon'
import type { AppConfig } from '../types'

type AppState = 'idle' | 'installing' | 'installed' | 'deleting' | 'error'

interface AppStatus {
  state: AppState
  message?: string
}

// Category metadata for display
const categoryInfo: Record<string, { label: string; icon: JSX.Element; description: string }> = {
  'media-manager': {
    label: 'Media Managers',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
      </svg>
    ),
    description: 'Organize and manage your movie and TV show collections'
  },
  'download-client': {
    label: 'Download Clients',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
      </svg>
    ),
    description: 'BitTorrent and Usenet clients for downloading content'
  },
  'media-server': {
    label: 'Media Servers',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
      </svg>
    ),
    description: 'Stream your media library to any device'
  },
  'request-manager': {
    label: 'Request Managers',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    ),
    description: 'Allow users to request new content'
  },
  'indexer': {
    label: 'Indexers',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
      </svg>
    ),
    description: 'Search and index content from various sources'
  },
  'monitoring': {
    label: 'Monitoring',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
      </svg>
    ),
    description: 'Metrics, logs, and dashboards'
  },
  'system': {
    label: 'System',
    icon: (
      <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </svg>
    ),
    description: 'Core system services'
  }
}

// Default category info for unknown categories
const defaultCategoryInfo = {
  label: 'Other Apps',
  icon: (
    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
    </svg>
  ),
  description: 'Additional applications'
}

// Category display order
const categoryOrder = ['media-manager', 'download-client', 'media-server', 'request-manager', 'indexer', 'monitoring', 'system']

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
    refetchInterval: 3000,
  })

  // Group apps by category
  const appsByCategory = useMemo(() => {
    if (!catalog) return {}

    const grouped: Record<string, AppConfig[]> = {}

    catalog.forEach(app => {
      const category = app.category || 'other'
      if (!grouped[category]) {
        grouped[category] = []
      }
      grouped[category].push(app)
    })

    return grouped
  }, [catalog])

  // Get sorted categories
  const sortedCategories = useMemo(() => {
    const categories = Object.keys(appsByCategory)
    return categories.sort((a, b) => {
      const aIndex = categoryOrder.indexOf(a)
      const bIndex = categoryOrder.indexOf(b)
      if (aIndex === -1 && bIndex === -1) return a.localeCompare(b)
      if (aIndex === -1) return 1
      if (bIndex === -1) return -1
      return aIndex - bIndex
    })
  }, [appsByCategory])

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
    const maxAttempts = 60
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
    const maxAttempts = 60
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

  const renderAppCard = (app: AppConfig) => {
    const isInstalled = installed?.includes(app.name)
    const appStatus = appStatuses[app.name] || { state: isInstalled ? 'installed' : 'idle' }
    const effectiveState = app.is_system ? 'installed' : appStatus.state

    return (
      <div
        key={app.name}
        className="bg-gray-800 rounded-xl border border-gray-700 hover:border-gray-600 transition-all duration-200 overflow-hidden"
      >

        <div className="p-5">
          <div className="flex items-start gap-4">
            <div className="flex-shrink-0">
              <AppIcon appName={app.name} size={48} />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-lg font-semibold text-white truncate">{app.display_name}</h3>
                <div className="flex items-center gap-1">
                  {app.is_system && (
                    <span className="flex-shrink-0 inline-flex items-center gap-1 bg-purple-500/20 text-purple-400 text-xs px-2 py-0.5 rounded-full">
                      <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                      </svg>
                      System
                    </span>
                  )}
                  {!app.is_system && effectiveState === 'installed' && (
                    <span className="flex-shrink-0 inline-flex items-center gap-1 bg-green-500/20 text-green-400 text-xs px-2 py-0.5 rounded-full">
                      <span className="w-1.5 h-1.5 bg-green-400 rounded-full"></span>
                      Installed
                    </span>
                  )}
                  {effectiveState === 'installing' && (
                    <span className="flex-shrink-0 inline-flex items-center gap-1 bg-blue-500/20 text-blue-400 text-xs px-2 py-0.5 rounded-full animate-pulse">
                      <span className="w-1.5 h-1.5 bg-blue-400 rounded-full"></span>
                      Installing
                    </span>
                  )}
                  {effectiveState === 'deleting' && (
                    <span className="flex-shrink-0 inline-flex items-center gap-1 bg-red-500/20 text-red-400 text-xs px-2 py-0.5 rounded-full animate-pulse">
                      <span className="w-1.5 h-1.5 bg-red-400 rounded-full"></span>
                      Removing
                    </span>
                  )}
                  {effectiveState === 'error' && (
                    <span className="flex-shrink-0 inline-flex items-center gap-1 bg-red-500/20 text-red-400 text-xs px-2 py-0.5 rounded-full">
                      <span className="w-1.5 h-1.5 bg-red-400 rounded-full"></span>
                      Error
                    </span>
                  )}
                </div>
              </div>
              <p className="text-sm text-gray-400 mt-1 line-clamp-2">{app.description}</p>
            </div>
          </div>

          <div className="flex gap-2 mt-4">
            {app.is_system && app.is_hidden ? (
              // System hidden apps (oauth2-proxy, kubarr-dashboard, loki, promtail) - no buttons
              <div className="w-full bg-gray-700 text-gray-400 text-sm font-medium py-2 px-4 rounded-lg text-center">
                Background Service
              </div>
            ) : app.is_system ? (
              // System apps with Open button (none currently, but for future use)
              <a
                href={`/${app.name}/`}
                target="_blank"
                rel="noopener noreferrer"
                className="w-full bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors text-center"
              >
                Open
              </a>
            ) : effectiveState === 'installed' ? (
              <>
                {!app.is_hidden && (
                  <a
                    href={`/${app.name}/`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex-1 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors text-center"
                  >
                    Open
                  </a>
                )}
                <button
                  onClick={() => deleteMutation.mutate(app.name)}
                  disabled={deleteMutation.isPending}
                  className={`${app.is_hidden ? 'w-full' : ''} bg-gray-700 hover:bg-red-600 disabled:bg-gray-800 disabled:cursor-not-allowed text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors`}
                  title="Uninstall"
                >
                  {app.is_hidden ? (
                    'Uninstall'
                  ) : (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  )}
                </button>
              </>
            ) : effectiveState === 'idle' || effectiveState === 'error' ? (
              <button
                onClick={() => installMutation.mutate(app.name)}
                className="w-full bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
              >
                {effectiveState === 'error' ? 'Retry Install' : 'Install'}
              </button>
            ) : (
              <button
                disabled
                className="w-full bg-gray-700 cursor-not-allowed text-gray-400 text-sm font-medium py-2 px-4 rounded-lg"
              >
                {effectiveState === 'installing' ? 'Installing...' : 'Removing...'}
              </button>
            )}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-8 pb-8">
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

      {/* Header */}
      <div className="border-b border-gray-800 pb-6">
        <h1 className="text-3xl font-bold text-white">App Marketplace</h1>
        <p className="text-gray-400 mt-2">Browse and install applications for your media server</p>
      </div>

      {/* Category Sections */}
      {sortedCategories.map(category => {
        const info = categoryInfo[category] || { ...defaultCategoryInfo, label: category.replace(/-/g, ' ').replace(/\b\w/g, l => l.toUpperCase()) }
        const apps = appsByCategory[category]

        return (
          <section key={category} className="space-y-4">
            {/* Category Header */}
            <div className="flex items-center gap-3">
              <div className="p-2 bg-gray-800 rounded-lg text-blue-400">
                {info.icon}
              </div>
              <div>
                <h2 className="text-xl font-semibold text-white">{info.label}</h2>
                <p className="text-sm text-gray-500">{info.description}</p>
              </div>
              <div className="ml-auto">
                <span className="text-sm text-gray-500">{apps.length} app{apps.length !== 1 ? 's' : ''}</span>
              </div>
            </div>

            {/* Apps Grid */}
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 3xl:grid-cols-6 gap-4">
              {apps.map(app => renderAppCard(app))}
            </div>
          </section>
        )
      })}
    </div>
  )
}
