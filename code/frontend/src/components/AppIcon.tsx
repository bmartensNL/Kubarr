import { useState, useEffect, memo } from 'react'

interface AppIconProps {
  appName: string
  size?: number
  className?: string
}

// Global in-memory cache for icon URLs and their loaded status
const iconCache = new Map<string, { loaded: boolean; dataUrl?: string; error?: boolean }>()

// Preload an icon and cache it
export async function preloadIcon(appName: string): Promise<void> {
  const cacheKey = appName.toLowerCase()

  // Skip if already cached
  if (iconCache.has(cacheKey)) return

  // Mark as loading
  iconCache.set(cacheKey, { loaded: false })

  try {
    const iconUrl = `/api/apps/catalog/${cacheKey}/icon`
    const response = await fetch(iconUrl)

    if (!response.ok) {
      iconCache.set(cacheKey, { loaded: true, error: true })
      return
    }

    const blob = await response.blob()
    const dataUrl = URL.createObjectURL(blob)
    iconCache.set(cacheKey, { loaded: true, dataUrl })
  } catch {
    iconCache.set(cacheKey, { loaded: true, error: true })
  }
}

// Preload multiple icons in parallel
export async function preloadIcons(appNames: string[]): Promise<void> {
  await Promise.all(appNames.map(preloadIcon))
}

// Get cached icon URL
function getCachedIcon(appName: string): { url: string | null; loading: boolean; error: boolean } {
  const cacheKey = appName.toLowerCase()
  const cached = iconCache.get(cacheKey)

  if (!cached) {
    // Not in cache, start loading
    preloadIcon(appName)
    return { url: null, loading: true, error: false }
  }

  if (!cached.loaded) {
    return { url: null, loading: true, error: false }
  }

  if (cached.error) {
    return { url: null, loading: false, error: true }
  }

  return { url: cached.dataUrl || null, loading: false, error: false }
}

function AppIconComponent({ appName, size = 40, className = '' }: AppIconProps) {
  const [, setRenderTrigger] = useState(0)
  const { url, loading, error } = getCachedIcon(appName)

  // If loading, set up a timer to re-check
  useEffect(() => {
    if (loading) {
      const checkLoaded = setInterval(() => {
        const cached = iconCache.get(appName.toLowerCase())
        if (cached?.loaded) {
          setRenderTrigger(prev => prev + 1)
          clearInterval(checkLoaded)
        }
      }, 50)

      return () => clearInterval(checkLoaded)
    }
  }, [loading, appName])

  // Show fallback if error
  if (error) {
    return (
      <div
        className={`flex items-center justify-center bg-gray-600 rounded-lg text-white font-bold ${className}`}
        style={{ width: size, height: size, fontSize: size * 0.5 }}
      >
        {appName.charAt(0).toUpperCase()}
      </div>
    )
  }

  // Show loading placeholder
  if (loading || !url) {
    return (
      <div
        className={`bg-gray-700 rounded-lg animate-pulse ${className}`}
        style={{ width: size, height: size }}
      />
    )
  }

  return (
    <img
      src={url}
      alt={`${appName} icon`}
      width={size}
      height={size}
      className={`rounded-lg ${className}`}
      loading="eager"
      decoding="async"
    />
  )
}

// Memoize the component to prevent unnecessary re-renders
export const AppIcon = memo(AppIconComponent)

export default AppIcon
