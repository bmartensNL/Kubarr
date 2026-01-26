import { useState } from 'react'
import { useAuth } from '../contexts/AuthContext'
import { useTheme } from '../contexts/ThemeContext'
import { User, Mail, Shield, Sun, Moon, Monitor, Check } from 'lucide-react'
import type { Theme } from '../api/users'

export default function AccountPage() {
  const { user, isAdmin } = useAuth()
  const { theme, setTheme } = useTheme()
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)

  const themes: { value: Theme; icon: typeof Sun; label: string; description: string }[] = [
    { value: 'light', icon: Sun, label: 'Light', description: 'Always use light mode' },
    { value: 'dark', icon: Moon, label: 'Dark', description: 'Always use dark mode' },
    { value: 'system', icon: Monitor, label: 'System', description: 'Follow your system preference' },
  ]

  const handleThemeChange = async (newTheme: Theme) => {
    setSaving(true)
    setSaved(false)
    await setTheme(newTheme)
    setSaving(false)
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  if (!user) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
      </div>
    )
  }

  return (
    <div className="max-w-2xl mx-auto space-y-8">
      <div>
        <h1 className="text-2xl font-bold">Account Settings</h1>
        <p className="text-gray-500 dark:text-gray-400 mt-1">Manage your account preferences</p>
      </div>

      {/* Profile Info */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm dark:shadow-none border border-gray-200 dark:border-gray-700 p-6">
        <h2 className="text-lg font-semibold mb-4">Profile</h2>
        <div className="space-y-4">
          <div className="flex items-center gap-4">
            <div className="w-10 h-10 rounded-full bg-blue-100 dark:bg-blue-900/30 flex items-center justify-center">
              <User size={20} className="text-blue-600 dark:text-blue-400" />
            </div>
            <div>
              <div className="text-sm text-gray-500 dark:text-gray-400">Username</div>
              <div className="font-medium">{user.username}</div>
            </div>
          </div>
          <div className="flex items-center gap-4">
            <div className="w-10 h-10 rounded-full bg-green-100 dark:bg-green-900/30 flex items-center justify-center">
              <Mail size={20} className="text-green-600 dark:text-green-400" />
            </div>
            <div>
              <div className="text-sm text-gray-500 dark:text-gray-400">Email</div>
              <div className="font-medium">{user.email}</div>
            </div>
          </div>
          <div className="flex items-center gap-4">
            <div className="w-10 h-10 rounded-full bg-purple-100 dark:bg-purple-900/30 flex items-center justify-center">
              <Shield size={20} className="text-purple-600 dark:text-purple-400" />
            </div>
            <div>
              <div className="text-sm text-gray-500 dark:text-gray-400">Role</div>
              <div className="font-medium">
                {isAdmin ? (
                  <span className="inline-flex items-center gap-1">
                    Administrator
                    <span className="px-1.5 py-0.5 text-xs bg-blue-600 text-white rounded font-medium">Admin</span>
                  </span>
                ) : (
                  user.roles?.length ? user.roles.map(r => r.name).join(', ') : 'User'
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Theme Preference */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm dark:shadow-none border border-gray-200 dark:border-gray-700 p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">Appearance</h2>
          {saving && (
            <span className="text-sm text-gray-500 dark:text-gray-400">Saving...</span>
          )}
          {saved && (
            <span className="text-sm text-green-600 dark:text-green-400 flex items-center gap-1">
              <Check size={16} />
              Saved
            </span>
          )}
        </div>
        <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
          Choose how Kubarr looks to you. Select a single theme, or sync with your system.
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {themes.map(({ value, icon: Icon, label, description }) => (
            <button
              key={value}
              onClick={() => handleThemeChange(value)}
              disabled={saving}
              className={`relative flex flex-col items-center gap-2 p-4 rounded-lg border-2 transition-all ${
                theme === value
                  ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                  : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600'
              }`}
            >
              {theme === value && (
                <div className="absolute top-2 right-2">
                  <Check size={16} className="text-blue-500" />
                </div>
              )}
              <Icon size={24} className={theme === value ? 'text-blue-500' : 'text-gray-500 dark:text-gray-400'} />
              <div className="text-sm font-medium">{label}</div>
              <div className="text-xs text-gray-500 dark:text-gray-400 text-center">{description}</div>
            </button>
          ))}
        </div>
      </div>

      {/* Account Info */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm dark:shadow-none border border-gray-200 dark:border-gray-700 p-6">
        <h2 className="text-lg font-semibold mb-4">Account Information</h2>
        <div className="space-y-3 text-sm">
          <div className="flex justify-between">
            <span className="text-gray-500 dark:text-gray-400">Account created</span>
            <span>{new Date(user.created_at).toLocaleDateString()}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-gray-500 dark:text-gray-400">Last updated</span>
            <span>{new Date(user.updated_at).toLocaleDateString()}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-gray-500 dark:text-gray-400">Status</span>
            <span className={user.is_active ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}>
              {user.is_active ? 'Active' : 'Inactive'}
            </span>
          </div>
        </div>
      </div>
    </div>
  )
}
