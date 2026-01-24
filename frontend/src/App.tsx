import { useState, useEffect } from 'react'
import { BrowserRouter, Routes, Route, Link, useLocation, Navigate } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import AppsPage from './pages/AppsPage'
import StoragePage from './pages/StoragePage'
import SettingsPage from './pages/SettingsPage'
import SetupPage from './pages/SetupPage'
import { AuthProvider, useAuth } from './contexts/AuthContext'
import { VersionFooter } from './components/VersionFooter'
import { setupApi } from './api/setup'
import { LayoutDashboard, Grid3X3, HardDrive, Settings, User, LogOut, Ship } from 'lucide-react'

function Navigation() {
  const { user, loading, isAdmin, logout } = useAuth()
  const location = useLocation()

  const handleLogout = () => {
    logout()
    window.location.href = '/oauth2/sign_out?rd=/auth/login'
  }

  const isActive = (path: string) => location.pathname === path

  return (
    <nav className="bg-gray-800 border-b border-gray-700">
      <div className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16">
        <div className="flex items-center justify-between h-16">
          <div className="flex items-center">
            <Link to="/" className="flex items-center space-x-2 text-xl font-bold hover:text-gray-300 transition-colors">
              <Ship size={28} className="text-blue-400" strokeWidth={2} />
              <span>Kubarr</span>
            </Link>
          </div>
          <div className="flex items-center space-x-1">
            <Link
              to="/"
              className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                isActive('/')
                  ? 'bg-gray-700 text-white'
                  : 'text-gray-300 hover:bg-gray-700 hover:text-white'
              }`}
            >
              <LayoutDashboard size={18} strokeWidth={2} />
              <span>Dashboard</span>
            </Link>
            <Link
              to="/apps"
              className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                isActive('/apps')
                  ? 'bg-gray-700 text-white'
                  : 'text-gray-300 hover:bg-gray-700 hover:text-white'
              }`}
            >
              <Grid3X3 size={18} strokeWidth={2} />
              <span>Apps</span>
            </Link>
            <Link
              to="/storage"
              className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                isActive('/storage')
                  ? 'bg-gray-700 text-white'
                  : 'text-gray-300 hover:bg-gray-700 hover:text-white'
              }`}
            >
              <HardDrive size={18} strokeWidth={2} />
              <span>Storage</span>
            </Link>
            {isAdmin && (
              <Link
                to="/settings"
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-colors ${
                  isActive('/settings')
                    ? 'bg-gray-700 text-white'
                    : 'text-gray-300 hover:bg-gray-700 hover:text-white'
                }`}
              >
                <Settings size={18} strokeWidth={2} />
                <span>Settings</span>
              </Link>
            )}
            <div className="flex items-center gap-3 ml-4 border-l border-gray-700 pl-4">
              {!loading && user && (
                <div className="flex items-center gap-2 text-sm text-gray-300">
                  <User size={18} strokeWidth={2} />
                  <span>{user.username}</span>
                  {user.is_admin && (
                    <span className="px-2 py-0.5 text-xs bg-blue-600 rounded-full font-medium">
                      Admin
                    </span>
                  )}
                </div>
              )}
              <button
                onClick={handleLogout}
                className="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium bg-red-600 hover:bg-red-700 transition-colors"
              >
                <LogOut size={18} strokeWidth={2} />
                <span>Logout</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </nav>
  )
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated, loading } = useAuth()

  if (loading) {
    return (
      <div className="h-screen bg-gray-900 text-white flex items-center justify-center">
        <div className="text-xl">Loading...</div>
      </div>
    )
  }

  // If not authenticated, oauth2-proxy should have already redirected to login
  // This is a fallback - redirect to oauth2-proxy sign_out to trigger re-auth
  if (!isAuthenticated) {
    window.location.href = '/oauth2/sign_out'
    return null
  }

  return <>{children}</>
}

function AppContent() {
  const location = useLocation()
  const [setupRequired, setSetupRequired] = useState<boolean | null>(null)
  const [setupCheckLoading, setSetupCheckLoading] = useState(true)

  const isSettingsPage = location.pathname === '/settings'
  const isSetupPage = location.pathname === '/setup'

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
      <div className="h-screen bg-gray-900 text-white flex items-center justify-center">
        <div className="text-xl">Loading...</div>
      </div>
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

  // Note: Login is handled by oauth2-proxy -> /auth/authorize (backend template)
  // No frontend login page needed

  return (
    <ProtectedRoute>
      <div className="h-screen bg-gray-900 text-white flex flex-col overflow-hidden">
        <Navigation />
        {isSettingsPage ? (
          <main className="flex-1 overflow-hidden">
            <Routes>
              <Route path="/settings" element={<SettingsPage />} />
            </Routes>
          </main>
        ) : (
          <main className="flex-1 overflow-auto pb-10">
            <div className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16 py-8">
              <Routes>
                <Route path="/" element={<Dashboard />} />
                <Route path="/apps" element={<AppsPage />} />
                <Route path="/storage" element={<StoragePage />} />
              </Routes>
            </div>
            <VersionFooter />
          </main>
        )}
      </div>
    </ProtectedRoute>
  )
}

function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <AppContent />
      </AuthProvider>
    </BrowserRouter>
  )
}

export default App
