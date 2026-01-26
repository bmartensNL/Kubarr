import { useState, useEffect } from 'react'
import { useAuth } from '../contexts/AuthContext'
import { useTheme } from '../contexts/ThemeContext'
import { User, Mail, Shield, Sun, Moon, Monitor, Check, Key, Smartphone, AlertTriangle, Eye, EyeOff, Loader2 } from 'lucide-react'
import type { Theme, TwoFactorStatusResponse, TwoFactorSetupResponse } from '../api/users'
import { changeOwnPassword, get2FAStatus, setup2FA, enable2FA, disable2FA } from '../api/users'

export default function AccountPage() {
  const { user, isAdmin } = useAuth()
  const { theme, setTheme } = useTheme()
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)

  // Password change state
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [showCurrentPassword, setShowCurrentPassword] = useState(false)
  const [showNewPassword, setShowNewPassword] = useState(false)
  const [passwordError, setPasswordError] = useState('')
  const [passwordSuccess, setPasswordSuccess] = useState('')
  const [changingPassword, setChangingPassword] = useState(false)

  // 2FA state
  const [twoFactorStatus, setTwoFactorStatus] = useState<TwoFactorStatusResponse | null>(null)
  const [loading2FA, setLoading2FA] = useState(true)
  const [setupData, setSetupData] = useState<TwoFactorSetupResponse | null>(null)
  const [verificationCode, setVerificationCode] = useState('')
  const [disablePassword, setDisablePassword] = useState('')
  const [twoFactorError, setTwoFactorError] = useState('')
  const [twoFactorSuccess, setTwoFactorSuccess] = useState('')
  const [processing2FA, setProcessing2FA] = useState(false)

  // Load 2FA status on mount
  useEffect(() => {
    const loadStatus = async () => {
      try {
        const status = await get2FAStatus()
        setTwoFactorStatus(status)
      } catch (err) {
        console.error('Failed to load 2FA status:', err)
      } finally {
        setLoading2FA(false)
      }
    }
    loadStatus()
  }, [])

  // Password change handler
  const handlePasswordChange = async (e: React.FormEvent) => {
    e.preventDefault()
    setPasswordError('')
    setPasswordSuccess('')

    if (newPassword.length < 8) {
      setPasswordError('New password must be at least 8 characters')
      return
    }
    if (newPassword !== confirmPassword) {
      setPasswordError('Passwords do not match')
      return
    }

    setChangingPassword(true)
    try {
      await changeOwnPassword({ current_password: currentPassword, new_password: newPassword })
      setPasswordSuccess('Password changed successfully')
      setCurrentPassword('')
      setNewPassword('')
      setConfirmPassword('')
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } }
      setPasswordError(error.response?.data?.error || 'Failed to change password')
    } finally {
      setChangingPassword(false)
    }
  }

  // 2FA setup handler
  const handleSetup2FA = async () => {
    setTwoFactorError('')
    setProcessing2FA(true)
    try {
      const data = await setup2FA()
      setSetupData(data)
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } }
      setTwoFactorError(error.response?.data?.error || 'Failed to set up 2FA')
    } finally {
      setProcessing2FA(false)
    }
  }

  // 2FA enable handler
  const handleEnable2FA = async () => {
    setTwoFactorError('')
    if (!verificationCode || verificationCode.length !== 6) {
      setTwoFactorError('Please enter a 6-digit code')
      return
    }

    setProcessing2FA(true)
    try {
      await enable2FA(verificationCode)
      setTwoFactorSuccess('Two-factor authentication enabled successfully')
      setSetupData(null)
      setVerificationCode('')
      const status = await get2FAStatus()
      setTwoFactorStatus(status)
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } }
      setTwoFactorError(error.response?.data?.error || 'Invalid verification code')
    } finally {
      setProcessing2FA(false)
    }
  }

  // 2FA disable handler
  const handleDisable2FA = async () => {
    setTwoFactorError('')
    if (!disablePassword) {
      setTwoFactorError('Please enter your password')
      return
    }

    setProcessing2FA(true)
    try {
      await disable2FA(disablePassword)
      setTwoFactorSuccess('Two-factor authentication disabled')
      setDisablePassword('')
      const status = await get2FAStatus()
      setTwoFactorStatus(status)
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } } }
      setTwoFactorError(error.response?.data?.error || 'Failed to disable 2FA')
    } finally {
      setProcessing2FA(false)
    }
  }

  // Cancel 2FA setup
  const cancelSetup = () => {
    setSetupData(null)
    setVerificationCode('')
    setTwoFactorError('')
  }

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

      {/* Security Section */}
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm dark:shadow-none border border-gray-200 dark:border-gray-700 p-6">
        <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
          <Key size={20} />
          Security
        </h2>

        {/* Password Change */}
        <div className="mb-8">
          <h3 className="text-md font-medium mb-3">Change Password</h3>
          <form onSubmit={handlePasswordChange} className="space-y-4 max-w-md">
            {passwordError && (
              <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-600 dark:text-red-400 text-sm">
                {passwordError}
              </div>
            )}
            {passwordSuccess && (
              <div className="p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg text-green-600 dark:text-green-400 text-sm flex items-center gap-2">
                <Check size={16} />
                {passwordSuccess}
              </div>
            )}
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                Current Password
              </label>
              <div className="relative">
                <input
                  type={showCurrentPassword ? 'text' : 'password'}
                  value={currentPassword}
                  onChange={(e) => setCurrentPassword(e.target.value)}
                  className="w-full px-3 py-2 pr-10 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                  required
                />
                <button
                  type="button"
                  onClick={() => setShowCurrentPassword(!showCurrentPassword)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
                >
                  {showCurrentPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                New Password
              </label>
              <div className="relative">
                <input
                  type={showNewPassword ? 'text' : 'password'}
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  className="w-full px-3 py-2 pr-10 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                  minLength={8}
                  required
                />
                <button
                  type="button"
                  onClick={() => setShowNewPassword(!showNewPassword)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
                >
                  {showNewPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">Minimum 8 characters</p>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                Confirm New Password
              </label>
              <input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                required
              />
            </div>
            <button
              type="submit"
              disabled={changingPassword}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white rounded-lg font-medium flex items-center gap-2"
            >
              {changingPassword && <Loader2 size={16} className="animate-spin" />}
              Change Password
            </button>
          </form>
        </div>

        {/* Two-Factor Authentication */}
        <div className="pt-6 border-t border-gray-200 dark:border-gray-700">
          <h3 className="text-md font-medium mb-3 flex items-center gap-2">
            <Smartphone size={18} />
            Two-Factor Authentication
          </h3>

          {loading2FA ? (
            <div className="flex items-center gap-2 text-gray-500">
              <Loader2 size={16} className="animate-spin" />
              Loading...
            </div>
          ) : (
            <>
              {twoFactorError && (
                <div className="p-3 mb-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-red-600 dark:text-red-400 text-sm">
                  {twoFactorError}
                </div>
              )}
              {twoFactorSuccess && (
                <div className="p-3 mb-4 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg text-green-600 dark:text-green-400 text-sm flex items-center gap-2">
                  <Check size={16} />
                  {twoFactorSuccess}
                </div>
              )}

              {/* Status Display */}
              <div className="flex items-center gap-3 mb-4">
                <span className="text-sm text-gray-600 dark:text-gray-400">Status:</span>
                {twoFactorStatus?.enabled ? (
                  <span className="inline-flex items-center gap-1 px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400 rounded text-sm font-medium">
                    <Check size={14} />
                    Enabled
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 px-2 py-1 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded text-sm font-medium">
                    Disabled
                  </span>
                )}
                {twoFactorStatus?.required_by_role && !twoFactorStatus?.enabled && (
                  <span className="inline-flex items-center gap-1 px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-400 rounded text-sm font-medium">
                    <AlertTriangle size={14} />
                    Required by your role
                  </span>
                )}
              </div>

              {/* Setup Flow */}
              {!twoFactorStatus?.enabled && !setupData && (
                <div>
                  <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
                    Add an extra layer of security to your account by enabling two-factor authentication.
                    You'll need an authenticator app like Google Authenticator, Authy, or 1Password.
                  </p>
                  <button
                    onClick={handleSetup2FA}
                    disabled={processing2FA}
                    className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white rounded-lg font-medium flex items-center gap-2"
                  >
                    {processing2FA && <Loader2 size={16} className="animate-spin" />}
                    Set Up Two-Factor Authentication
                  </button>
                </div>
              )}

              {/* QR Code and Verification */}
              {setupData && (
                <div className="space-y-4">
                  <div className="p-4 bg-gray-50 dark:bg-gray-900 rounded-lg">
                    <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
                      Scan this QR code with your authenticator app, then enter the 6-digit code below to verify.
                    </p>
                    <div className="flex flex-col sm:flex-row gap-6 items-start">
                      <div className="bg-white p-2 rounded-lg">
                        <img
                          src={setupData.qr_code_base64}
                          alt="2FA QR Code"
                          className="w-48 h-48"
                        />
                      </div>
                      <div className="flex-1 space-y-3">
                        <div>
                          <label className="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">
                            Manual Entry Key
                          </label>
                          <code className="block px-3 py-2 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded text-sm font-mono break-all">
                            {setupData.secret}
                          </code>
                        </div>
                        <div>
                          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                            Verification Code
                          </label>
                          <input
                            type="text"
                            value={verificationCode}
                            onChange={(e) => setVerificationCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
                            placeholder="000000"
                            maxLength={6}
                            className="w-32 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 focus:border-transparent text-center font-mono text-lg tracking-wider"
                          />
                        </div>
                      </div>
                    </div>
                  </div>
                  <div className="flex gap-3">
                    <button
                      onClick={handleEnable2FA}
                      disabled={processing2FA || verificationCode.length !== 6}
                      className="px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-green-400 text-white rounded-lg font-medium flex items-center gap-2"
                    >
                      {processing2FA && <Loader2 size={16} className="animate-spin" />}
                      Verify & Enable
                    </button>
                    <button
                      onClick={cancelSetup}
                      className="px-4 py-2 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 rounded-lg font-medium"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              )}

              {/* Disable 2FA */}
              {twoFactorStatus?.enabled && (
                <div className="space-y-4">
                  <p className="text-sm text-gray-600 dark:text-gray-400">
                    To disable two-factor authentication, enter your password below.
                  </p>
                  {twoFactorStatus.required_by_role && (
                    <div className="p-3 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg text-amber-700 dark:text-amber-400 text-sm flex items-center gap-2">
                      <AlertTriangle size={16} />
                      Your role requires 2FA. You cannot disable it while assigned to this role.
                    </div>
                  )}
                  <div className="flex gap-3 items-end">
                    <div>
                      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                        Password
                      </label>
                      <input
                        type="password"
                        value={disablePassword}
                        onChange={(e) => setDisablePassword(e.target.value)}
                        className="w-48 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                        disabled={twoFactorStatus.required_by_role}
                      />
                    </div>
                    <button
                      onClick={handleDisable2FA}
                      disabled={processing2FA || twoFactorStatus.required_by_role || !disablePassword}
                      className="px-4 py-2 bg-red-600 hover:bg-red-700 disabled:bg-red-400 disabled:cursor-not-allowed text-white rounded-lg font-medium flex items-center gap-2"
                    >
                      {processing2FA && <Loader2 size={16} className="animate-spin" />}
                      Disable 2FA
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
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
