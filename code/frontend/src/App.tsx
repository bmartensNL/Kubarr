import { useState, useEffect } from 'react'
import { BrowserRouter, Routes, Route, Link, useLocation, Navigate } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import AppsPage from './pages/AppsPage'
import StoragePage from './pages/StoragePage'
import LogsPage from './pages/LogsPage'
import MonitoringPage from './pages/MonitoringPage'
import NetworkingPage from './pages/NetworkingPage'
import SecurityPage from './pages/SecurityPage'
import SettingsPage from './pages/SettingsPage'
import AccountPage from './pages/AccountPage'
import SetupPage from './pages/SetupPage'
import LoginPage from './pages/LoginPage'
import NotFoundPage from './pages/NotFoundPage'
import AppErrorPage from './pages/AppErrorPage'
import { AuthProvider, useAuth } from './contexts/AuthContext'
import { ThemeProvider, useTheme } from './contexts/ThemeContext'
import { MonitoringProvider } from './contexts/MonitoringContext'
import { VersionFooter } from './components/VersionFooter'
import { PageTransition } from './components/PageTransition'
import { setupApi } from './api/setup'
import { sessionLogout } from './api/auth'
import { Grid3X3, HardDrive, FileText, Activity, Settings, User, LogOut, Ship, ChevronDown, Sun, Moon, Monitor, Network, Menu, X, Shield, Bell, Check, Trash2, AlertCircle, Info, AlertTriangle } from 'lucide-react'
import { notificationsApi, Notification } from './api/notifications'

function ThemeToggle() {
  const { theme, resolvedTheme, setTheme } = useTheme()
  const [dropdownOpen, setDropdownOpen] = useState(false)

  const themes = [
    { value: 'light' as const, icon: Sun, label: 'Light' },
    { value: 'dark' as const, icon: Moon, label: 'Dark' },
    { value: 'system' as const, icon: Monitor, label: 'System' },
  ]

  // Show Sun/Moon based on actual displayed theme, not the setting
  const ButtonIcon = resolvedTheme === 'dark' ? Moon : Sun
  const currentTheme = themes.find(t => t.value === theme) || themes[2]

  return (
    <div className="relative">
      <button
        onClick={() => setDropdownOpen(!dropdownOpen)}
        onBlur={() => setTimeout(() => setDropdownOpen(false), 150)}
        className="flex items-center justify-center w-9 h-9 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
        title={`Theme: ${currentTheme.label}`}
      >
        <ButtonIcon size={18} strokeWidth={2} />
      </button>
      {dropdownOpen && (
        <div className="absolute top-full right-0 mt-1 w-36 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg z-50">
          {themes.map(({ value, icon: Icon, label }) => (
            <button
              key={value}
              onClick={() => {
                setTheme(value)
                setDropdownOpen(false)
              }}
              className={`flex items-center gap-2 w-full px-3 py-2 text-sm transition-colors ${
                theme === value
                  ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                  : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
              } first:rounded-t-md last:rounded-b-md`}
            >
              <Icon size={16} strokeWidth={2} />
              <span>{label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function NotificationInbox() {
  const { isAuthenticated } = useAuth()
  const [dropdownOpen, setDropdownOpen] = useState(false)
  const [notifications, setNotifications] = useState<Notification[]>([])
  const [unreadCount, setUnreadCount] = useState(0)
  const [loading, setLoading] = useState(false)

  // Fetch unread count on mount and periodically
  useEffect(() => {
    if (!isAuthenticated) return

    const fetchUnreadCount = async () => {
      try {
        const count = await notificationsApi.getUnreadCount()
        setUnreadCount(count)
      } catch {
        // Silently fail - not critical
      }
    }

    fetchUnreadCount()
    const interval = setInterval(fetchUnreadCount, 30000) // Every 30 seconds
    return () => clearInterval(interval)
  }, [isAuthenticated])

  // Fetch notifications when dropdown opens
  const handleOpen = async () => {
    if (!dropdownOpen) {
      setLoading(true)
      try {
        const response = await notificationsApi.getInbox(10, 0)
        setNotifications(response.notifications)
        setUnreadCount(response.unread)
      } catch {
        // Silently fail
      } finally {
        setLoading(false)
      }
    }
    setDropdownOpen(!dropdownOpen)
  }

  const handleMarkAsRead = async (id: number, e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await notificationsApi.markAsRead(id)
      setNotifications(prev => prev.map(n => n.id === id ? { ...n, read: true } : n))
      setUnreadCount(prev => Math.max(0, prev - 1))
    } catch {
      // Silently fail
    }
  }

  const handleMarkAllAsRead = async () => {
    try {
      await notificationsApi.markAllAsRead()
      setNotifications(prev => prev.map(n => ({ ...n, read: true })))
      setUnreadCount(0)
    } catch {
      // Silently fail
    }
  }

  const handleDelete = async (id: number, e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await notificationsApi.deleteNotification(id)
      const wasUnread = notifications.find(n => n.id === id && !n.read)
      setNotifications(prev => prev.filter(n => n.id !== id))
      if (wasUnread) {
        setUnreadCount(prev => Math.max(0, prev - 1))
      }
    } catch {
      // Silently fail
    }
  }

  const getSeverityIcon = (severity: string) => {
    switch (severity) {
      case 'critical':
        return <AlertCircle size={16} className="text-red-500" />
      case 'warning':
        return <AlertTriangle size={16} className="text-yellow-500" />
      default:
        return <Info size={16} className="text-blue-500" />
    }
  }

  const formatTime = (dateStr: string) => {
    const date = new Date(dateStr)
    const now = new Date()
    const diff = now.getTime() - date.getTime()
    const minutes = Math.floor(diff / 60000)
    const hours = Math.floor(diff / 3600000)
    const days = Math.floor(diff / 86400000)

    if (minutes < 1) return 'Just now'
    if (minutes < 60) return `${minutes}m ago`
    if (hours < 24) return `${hours}h ago`
    if (days < 7) return `${days}d ago`
    return date.toLocaleDateString()
  }

  if (!isAuthenticated) return null

  return (
    <div className="relative">
      <button
        onClick={handleOpen}
        onBlur={() => setTimeout(() => setDropdownOpen(false), 200)}
        className="relative flex items-center justify-center w-9 h-9 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
        title="Notifications"
      >
        <Bell size={18} strokeWidth={2} />
        {unreadCount > 0 && (
          <span className="absolute -top-1 -right-1 flex items-center justify-center min-w-[18px] h-[18px] px-1 text-xs font-bold text-white bg-red-500 rounded-full">
            {unreadCount > 99 ? '99+' : unreadCount}
          </span>
        )}
      </button>
      {dropdownOpen && (
        <div className="absolute top-full right-0 mt-1 w-80 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg z-50 overflow-hidden">
          <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
            <h3 className="font-semibold text-gray-900 dark:text-white">Notifications</h3>
            {unreadCount > 0 && (
              <button
                onClick={handleMarkAllAsRead}
                className="text-xs text-blue-600 dark:text-blue-400 hover:underline"
              >
                Mark all as read
              </button>
            )}
          </div>
          <div className="max-h-80 overflow-y-auto">
            {loading ? (
              <div className="flex items-center justify-center py-8">
                <div className="text-gray-500 dark:text-gray-400">Loading...</div>
              </div>
            ) : notifications.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-8 text-gray-500 dark:text-gray-400">
                <Bell size={24} className="mb-2 opacity-50" />
                <span>No notifications</span>
              </div>
            ) : (
              notifications.map(notification => (
                <div
                  key={notification.id}
                  className={`px-4 py-3 border-b border-gray-100 dark:border-gray-700 last:border-b-0 hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors ${
                    !notification.read ? 'bg-blue-50 dark:bg-blue-900/20' : ''
                  }`}
                >
                  <div className="flex items-start gap-3">
                    <div className="mt-0.5">
                      {getSeverityIcon(notification.severity)}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center justify-between gap-2">
                        <h4 className={`text-sm font-medium truncate ${
                          !notification.read
                            ? 'text-gray-900 dark:text-white'
                            : 'text-gray-700 dark:text-gray-300'
                        }`}>
                          {notification.title}
                        </h4>
                        <div className="flex items-center gap-1 flex-shrink-0">
                          {!notification.read && (
                            <button
                              onClick={(e) => handleMarkAsRead(notification.id, e)}
                              className="p-1 text-gray-400 hover:text-green-500 transition-colors"
                              title="Mark as read"
                            >
                              <Check size={14} />
                            </button>
                          )}
                          <button
                            onClick={(e) => handleDelete(notification.id, e)}
                            className="p-1 text-gray-400 hover:text-red-500 transition-colors"
                            title="Delete"
                          >
                            <Trash2 size={14} />
                          </button>
                        </div>
                      </div>
                      <p className="text-xs text-gray-500 dark:text-gray-400 mt-1 line-clamp-2">
                        {notification.message}
                      </p>
                      <span className="text-xs text-gray-400 dark:text-gray-500 mt-1 block">
                        {formatTime(notification.created_at)}
                      </span>
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
          <div className="px-4 py-2 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50">
            <Link
              to="/account"
              className="text-xs text-blue-600 dark:text-blue-400 hover:underline"
              onClick={() => setDropdownOpen(false)}
            >
              Notification preferences
            </Link>
          </div>
        </div>
      )}
    </div>
  )
}

function UserMenu() {
  const { user, loading, logout } = useAuth()
  const [dropdownOpen, setDropdownOpen] = useState(false)
  const location = useLocation()

  const handleLogout = async () => {
    try {
      await sessionLogout()
    } catch (e) {
      // Ignore errors - cookie will be cleared anyway
    }
    logout()
    window.location.href = '/login'
  }

  if (loading || !user) return null

  const isAccountActive = location.pathname === '/account'

  return (
    <div className="relative">
      <button
        onClick={() => setDropdownOpen(!dropdownOpen)}
        onBlur={() => setTimeout(() => setDropdownOpen(false), 150)}
        className={`flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
          isAccountActive
            ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
        }`}
      >
        <User size={18} strokeWidth={2} />
        <span>{user.username}</span>
        <ChevronDown size={16} strokeWidth={2} className={`transition-transform ${dropdownOpen ? 'rotate-180' : ''}`} />
      </button>
      {dropdownOpen && (
        <div className="absolute top-full right-0 mt-1 w-48 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg z-50">
          <Link
            to="/account"
            className="flex items-center gap-2 px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-t-md"
          >
            <Settings size={16} strokeWidth={2} />
            <span>Account Settings</span>
          </Link>
          <button
            onClick={handleLogout}
            className="flex items-center gap-2 w-full px-4 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-b-md"
          >
            <LogOut size={16} strokeWidth={2} />
            <span>Logout</span>
          </button>
        </div>
      )}
    </div>
  )
}

function Navigation() {
  const { hasPermission, logout } = useAuth()
  const location = useLocation()
  const [systemDropdownOpen, setSystemDropdownOpen] = useState(false)
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const [mobileSystemOpen, setMobileSystemOpen] = useState(false)

  const isActive = (path: string) => location.pathname === path

  const handleLogout = async () => {
    setMobileMenuOpen(false)
    try {
      await sessionLogout()
    } catch (e) {
      // Ignore errors
    }
    logout()
    window.location.href = '/login'
  }

  // Check which system menu items are visible
  const canViewResources = hasPermission('monitoring.view')
  const canViewStorage = hasPermission('storage.view')
  const canViewLogs = hasPermission('logs.view')
  const canViewApps = hasPermission('apps.view')
  const canViewSecurity = hasPermission('audit.view')

  // Show System dropdown if user has any system permission
  const hasAnySystemPermission = canViewResources || canViewStorage || canViewLogs || canViewSecurity
  const isSystemActive = ['/storage', '/logs', '/resources', '/networking', '/security'].includes(location.pathname)

  // Show Settings if user has any settings/users/roles permission
  const canViewSettings = hasPermission('settings.view') || hasPermission('users.view') || hasPermission('roles.view')

  // Close mobile menu on navigation
  useEffect(() => {
    setMobileMenuOpen(false)
    setMobileSystemOpen(false)
  }, [location.pathname])

  return (
    <nav className="bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
      <div className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16">
        <div className="flex items-center justify-between h-16">
          {/* Logo */}
          <div className="flex items-center">
            <Link to="/" className="flex items-center space-x-2 text-xl font-bold text-gray-900 dark:text-white hover:text-gray-600 dark:hover:text-gray-300 transition-colors">
              <Ship size={28} className="text-blue-500 dark:text-blue-400" strokeWidth={2} />
              <span>Kubarr</span>
            </Link>
          </div>

          {/* Desktop Navigation */}
          <div className="hidden md:flex items-center space-x-1">
            {canViewApps && (
              <Link
                to="/apps"
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                  isActive('/apps')
                    ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                    : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                }`}
              >
                <Grid3X3 size={18} strokeWidth={2} />
                <span>Apps</span>
              </Link>
            )}
            {hasAnySystemPermission && (
              <div className="relative">
                <button
                  onClick={() => setSystemDropdownOpen(!systemDropdownOpen)}
                  onBlur={() => setTimeout(() => setSystemDropdownOpen(false), 150)}
                  className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                    isSystemActive
                      ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                  }`}
                >
                  <Activity size={18} strokeWidth={2} />
                  <span>System</span>
                  <ChevronDown size={16} strokeWidth={2} className={`transition-transform ${systemDropdownOpen ? 'rotate-180' : ''}`} />
                </button>
                {systemDropdownOpen && (
                  <div className="absolute top-full left-0 mt-1 w-48 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg z-50">
                    {canViewResources && (
                      <Link
                        to="/resources"
                        className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
                          isActive('/resources')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                        } first:rounded-t-md last:rounded-b-md`}
                      >
                        <Activity size={16} strokeWidth={2} />
                        <span>Resources</span>
                      </Link>
                    )}
                    {canViewResources && (
                      <Link
                        to="/networking"
                        className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
                          isActive('/networking')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                        } first:rounded-t-md last:rounded-b-md`}
                      >
                        <Network size={16} strokeWidth={2} />
                        <span>Networking</span>
                      </Link>
                    )}
                    {canViewStorage && (
                      <Link
                        to="/storage"
                        className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
                          isActive('/storage')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                        } first:rounded-t-md last:rounded-b-md`}
                      >
                        <HardDrive size={16} strokeWidth={2} />
                        <span>Storage</span>
                      </Link>
                    )}
                    {canViewLogs && (
                      <Link
                        to="/logs"
                        className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
                          isActive('/logs')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                        } first:rounded-t-md last:rounded-b-md`}
                      >
                        <FileText size={16} strokeWidth={2} />
                        <span>Logs</span>
                      </Link>
                    )}
                    {canViewSecurity && (
                      <Link
                        to="/security"
                        className={`flex items-center gap-2 px-4 py-2 text-sm transition-colors ${
                          isActive('/security')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                        } first:rounded-t-md last:rounded-b-md`}
                      >
                        <Shield size={16} strokeWidth={2} />
                        <span>Security</span>
                      </Link>
                    )}
                  </div>
                )}
              </div>
            )}
            {canViewSettings && (
              <Link
                to="/settings"
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                  isActive('/settings')
                    ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                    : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                }`}
              >
                <Settings size={18} strokeWidth={2} />
                <span>Settings</span>
              </Link>
            )}
            <div className="flex items-center gap-2 ml-4 border-l border-gray-200 dark:border-gray-700 pl-4">
              <NotificationInbox />
              <UserMenu />
              <ThemeToggle />
            </div>
          </div>

          {/* Mobile: Theme toggle and hamburger menu */}
          <div className="flex md:hidden items-center gap-2">
            <ThemeToggle />
            <button
              onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
              className="flex items-center justify-center w-10 h-10 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
              aria-label="Toggle menu"
            >
              {mobileMenuOpen ? <X size={24} strokeWidth={2} /> : <Menu size={24} strokeWidth={2} />}
            </button>
          </div>
        </div>
      </div>

      {/* Mobile Menu Overlay */}
      {mobileMenuOpen && (
        <div className="md:hidden border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
          <div className="px-4 py-3 space-y-1">
            {/* Dashboard link */}
            <Link
              to="/"
              className={`flex items-center gap-3 px-4 py-3 rounded-md text-base font-medium transition-colors ${
                isActive('/')
                  ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                  : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
              }`}
            >
              <Ship size={20} strokeWidth={2} />
              <span>Dashboard</span>
            </Link>

            {canViewApps && (
              <Link
                to="/apps"
                className={`flex items-center gap-3 px-4 py-3 rounded-md text-base font-medium transition-colors ${
                  isActive('/apps')
                    ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                    : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                }`}
              >
                <Grid3X3 size={20} strokeWidth={2} />
                <span>Apps</span>
              </Link>
            )}

            {/* System submenu */}
            {hasAnySystemPermission && (
              <div>
                <button
                  onClick={() => setMobileSystemOpen(!mobileSystemOpen)}
                  className={`flex items-center justify-between w-full px-4 py-3 rounded-md text-base font-medium transition-colors ${
                    isSystemActive
                      ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <Activity size={20} strokeWidth={2} />
                    <span>System</span>
                  </div>
                  <ChevronDown size={20} strokeWidth={2} className={`transition-transform ${mobileSystemOpen ? 'rotate-180' : ''}`} />
                </button>
                {mobileSystemOpen && (
                  <div className="ml-6 mt-1 space-y-1">
                    {canViewResources && (
                      <Link
                        to="/resources"
                        className={`flex items-center gap-3 px-4 py-3 rounded-md text-base transition-colors ${
                          isActive('/resources')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700'
                        }`}
                      >
                        <Activity size={18} strokeWidth={2} />
                        <span>Resources</span>
                      </Link>
                    )}
                    {canViewResources && (
                      <Link
                        to="/networking"
                        className={`flex items-center gap-3 px-4 py-3 rounded-md text-base transition-colors ${
                          isActive('/networking')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700'
                        }`}
                      >
                        <Network size={18} strokeWidth={2} />
                        <span>Networking</span>
                      </Link>
                    )}
                    {canViewStorage && (
                      <Link
                        to="/storage"
                        className={`flex items-center gap-3 px-4 py-3 rounded-md text-base transition-colors ${
                          isActive('/storage')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700'
                        }`}
                      >
                        <HardDrive size={18} strokeWidth={2} />
                        <span>Storage</span>
                      </Link>
                    )}
                    {canViewLogs && (
                      <Link
                        to="/logs"
                        className={`flex items-center gap-3 px-4 py-3 rounded-md text-base transition-colors ${
                          isActive('/logs')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700'
                        }`}
                      >
                        <FileText size={18} strokeWidth={2} />
                        <span>Logs</span>
                      </Link>
                    )}
                    {canViewSecurity && (
                      <Link
                        to="/security"
                        className={`flex items-center gap-3 px-4 py-3 rounded-md text-base transition-colors ${
                          isActive('/security')
                            ? 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'
                            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700'
                        }`}
                      >
                        <Shield size={18} strokeWidth={2} />
                        <span>Security</span>
                      </Link>
                    )}
                  </div>
                )}
              </div>
            )}

            {canViewSettings && (
              <Link
                to="/settings"
                className={`flex items-center gap-3 px-4 py-3 rounded-md text-base font-medium transition-colors ${
                  isActive('/settings')
                    ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                    : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                }`}
              >
                <Settings size={20} strokeWidth={2} />
                <span>Settings</span>
              </Link>
            )}

            {/* Divider */}
            <div className="border-t border-gray-200 dark:border-gray-700 my-2" />

            {/* Account link */}
            <Link
              to="/account"
              className={`flex items-center gap-3 px-4 py-3 rounded-md text-base font-medium transition-colors ${
                isActive('/account')
                  ? 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                  : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
              }`}
            >
              <User size={20} strokeWidth={2} />
              <span>Account</span>
            </Link>

            {/* Logout button */}
            <button
              onClick={handleLogout}
              className="flex items-center gap-3 w-full px-4 py-3 rounded-md text-base font-medium text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
            >
              <LogOut size={20} strokeWidth={2} />
              <span>Logout</span>
            </button>
          </div>
        </div>
      )}
    </nav>
  )
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated, loading } = useAuth()

  if (loading) {
    return (
      <div className="h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-white flex items-center justify-center">
        <div className="text-xl">Loading...</div>
      </div>
    )
  }

  // If not authenticated, redirect to login page
  if (!isAuthenticated) {
    window.location.href = '/login'
    return null
  }

  return <>{children}</>
}

function AppContent() {
  const location = useLocation()
  const [setupRequired, setSetupRequired] = useState<boolean | null>(null)
  const [setupCheckLoading, setSetupCheckLoading] = useState(true)

  const isSettingsPage = location.pathname === '/settings'
  const isAccountPage = location.pathname === '/account'
  const isLogsPage = location.pathname === '/logs'
  const isSetupPage = location.pathname === '/setup'
  const isLoginPage = location.pathname === '/login'

  // Check if setup is required on mount
  useEffect(() => {
    const checkSetup = async () => {
      try {
        const { setup_required } = await setupApi.checkRequired()
        setSetupRequired(setup_required)
      } catch (err) {
        // If we can't check, assume setup is not required
        setSetupRequired(false)
      } finally {
        setSetupCheckLoading(false)
      }
    }
    checkSetup()
  }, [])

  // Show loading while checking setup
  if (setupCheckLoading) {
    return (
      <div className="h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-white flex items-center justify-center">
        <div className="text-xl">Loading...</div>
      </div>
    )
  }

  // Render login page without navigation and without protection
  if (isLoginPage) {
    return (
      <Routes>
        <Route path="/login" element={<LoginPage />} />
      </Routes>
    )
  }

  // Redirect to setup if required and not already on setup page
  if (setupRequired && !isSetupPage) {
    return <Navigate to="/setup" replace />
  }

  // Render setup page without navigation
  if (isSetupPage) {
    return (
      <Routes>
        <Route path="/setup" element={<SetupPage />} />
      </Routes>
    )
  }

  return (
    <ProtectedRoute>
      <MonitoringProvider>
      <div className="h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-white flex flex-col overflow-hidden">
        <Navigation />
        {isSettingsPage || isAccountPage || isLogsPage ? (
          <main className="flex-1 overflow-hidden p-6">
            <PageTransition className="h-full">
              <Routes>
                <Route path="/settings" element={<SettingsPage />} />
                <Route path="/account" element={<AccountPage />} />
                <Route path="/logs" element={<LogsPage />} />
                <Route path="*" element={<NotFoundPage />} />
              </Routes>
            </PageTransition>
          </main>
        ) : (
          <main className="flex-1 overflow-auto pb-10">
            <PageTransition>
              <div className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16 py-8">
                <Routes>
                  <Route path="/" element={<Dashboard />} />
                  <Route path="/apps" element={<AppsPage />} />
                  <Route path="/storage" element={<StoragePage />} />
                  <Route path="/resources" element={<MonitoringPage />} />
                  <Route path="/networking" element={<NetworkingPage />} />
                  <Route path="/security" element={<SecurityPage />} />
                  <Route path="/app-error" element={<AppErrorPage />} />
                  <Route path="*" element={<NotFoundPage />} />
                </Routes>
              </div>
            </PageTransition>
            <VersionFooter />
          </main>
        )}
      </div>
      </MonitoringProvider>
    </ProtectedRoute>
  )
}

function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <ThemeProvider>
          <AppContent />
        </ThemeProvider>
      </AuthProvider>
    </BrowserRouter>
  )
}

export default App
