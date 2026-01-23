import { BrowserRouter, Routes, Route, Link } from 'react-router-dom'
import { useEffect, useState } from 'react'
import Dashboard from './pages/Dashboard'
import AppsPage from './pages/AppsPage'
import UsersPage from './pages/UsersPage'
import apiClient from './api/client'

interface User {
  id: number
  username: string
  email: string
  is_admin: boolean
  is_active: boolean
  is_approved: boolean
}

function Navigation() {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const fetchUser = async () => {
      try {
        const response = await apiClient.get<User>('/api/users/me')
        setUser(response.data)
      } catch (error) {
        console.error('Failed to fetch user:', error)
      } finally {
        setLoading(false)
      }
    }
    fetchUser()
  }, [])

  const logout = () => {
    window.location.href = '/oauth2/sign_out?rd=/auth/login'
  }

  const isAdmin = user?.is_admin || false

  return (
    <nav className="bg-gray-800 border-b border-gray-700">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
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
                onClick={logout}
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
      <div className="min-h-screen bg-gray-900 text-white">
        <Navigation />
        <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/apps" element={<AppsPage />} />
            <Route path="/users" element={<UsersPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}

export default App
