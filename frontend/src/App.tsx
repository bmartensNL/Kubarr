import { BrowserRouter, Routes, Route, Link } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import AppsPage from './pages/AppsPage'
import UsersPage from './pages/UsersPage'
import { AuthProvider, useAuth } from './contexts/AuthContext'
import { VersionFooter } from './components/VersionFooter'

function Navigation() {
  const { user, loading, isAdmin, logout } = useAuth()

  const handleLogout = () => {
    logout()
    window.location.href = '/oauth2/sign_out?rd=/auth/login'
  }

  return (
    <nav className="bg-gray-800 border-b border-gray-700">
      <div className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16">
        <div className="flex items-center justify-between h-16">
          <div className="flex items-center">
            <Link to="/" className="text-xl font-bold hover:text-gray-300">
              Kubarr Dashboard
            </Link>
          </div>
          <div className="flex items-center space-x-4">
            <Link
              to="/"
              className="px-3 py-2 rounded-md text-sm font-medium hover:bg-gray-700"
            >
              Dashboard
            </Link>
            <Link
              to="/apps"
              className="px-3 py-2 rounded-md text-sm font-medium hover:bg-gray-700"
            >
              Apps
            </Link>
            {isAdmin && (
              <Link
                to="/users"
                className="px-3 py-2 rounded-md text-sm font-medium hover:bg-gray-700"
              >
                Users
              </Link>
            )}
            <div className="flex items-center space-x-4 ml-4 border-l border-gray-700 pl-4">
              {!loading && user && (
                <span className="text-sm text-gray-300">
                  {user.username}
                  {user.is_admin && <span className="ml-2 px-2 py-1 text-xs bg-blue-600 rounded">Admin</span>}
                </span>
              )}
              <button
                onClick={handleLogout}
                className="px-3 py-2 rounded-md text-sm font-medium bg-red-600 hover:bg-red-700"
              >
                Logout
              </button>
            </div>
          </div>
        </div>
      </div>
    </nav>
  )
}

function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <div className="min-h-screen bg-gray-900 text-white pb-10">
          <Navigation />
          <main className="w-full px-4 sm:px-6 lg:px-8 xl:px-12 2xl:px-16 py-8">
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/apps" element={<AppsPage />} />
              <Route path="/users" element={<UsersPage />} />
            </Routes>
          </main>
          <VersionFooter />
        </div>
      </AuthProvider>
    </BrowserRouter>
  )
}

export default App
