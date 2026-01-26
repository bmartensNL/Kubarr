import { useState, FormEvent, useEffect, useRef } from 'react'
import { Ship, Shield, ArrowLeft, Loader2 } from 'lucide-react'
import { sessionLogin, verify2FA, SessionLoginResponse } from '../api/auth'

type LoginStep = 'credentials' | '2fa_required' | '2fa_setup_required'

export default function LoginPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  // 2FA state
  const [step, setStep] = useState<LoginStep>('credentials')
  const [challengeToken, setChallengeToken] = useState<string | null>(null)
  const [totpCode, setTotpCode] = useState('')
  const totpInputRef = useRef<HTMLInputElement>(null)

  // Focus TOTP input when entering 2FA step
  useEffect(() => {
    if (step === '2fa_required' && totpInputRef.current) {
      totpInputRef.current.focus()
    }
  }, [step])

  const handleCredentialsSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    setLoading(true)

    try {
      const response: SessionLoginResponse = await sessionLogin({ username, password })

      switch (response.status) {
        case 'success':
          // Session cookie is set by backend - just redirect
          window.location.href = '/'
          break
        case '2fa_required':
          // Need to enter TOTP code
          setChallengeToken(response.challenge_token || null)
          setStep('2fa_required')
          setLoading(false)
          break
        case '2fa_setup_required':
          // User needs to set up 2FA first
          setStep('2fa_setup_required')
          setLoading(false)
          break
        default:
          throw new Error('Unexpected response')
      }
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } }, message?: string }
      setError(error.response?.data?.error || error.message || 'Login failed')
      setLoading(false)
    }
  }

  const handleTotpSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!challengeToken || totpCode.length !== 6) return

    setError(null)
    setLoading(true)

    try {
      const response = await verify2FA({ challenge_token: challengeToken, code: totpCode })

      if (response.status === 'success') {
        // Session cookie is set by backend - just redirect
        window.location.href = '/'
      } else {
        throw new Error('Unexpected response')
      }
    } catch (err: unknown) {
      const error = err as { response?: { data?: { error?: string } }, message?: string }
      setError(error.response?.data?.error || error.message || 'Verification failed')
      setTotpCode('')
      setLoading(false)
    }
  }

  const handleBack = () => {
    setStep('credentials')
    setChallengeToken(null)
    setTotpCode('')
    setError(null)
    setPassword('') // Clear password for security
  }

  // 2FA Setup Required View
  if (step === '2fa_setup_required') {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center px-4">
        <div className="max-w-md w-full space-y-8">
          <div className="text-center">
            <div className="flex justify-center mb-4">
              <div className="p-3 bg-amber-100 dark:bg-amber-900/30 rounded-full">
                <Shield size={32} className="text-amber-600 dark:text-amber-400" />
              </div>
            </div>
            <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
              Two-Factor Authentication Required
            </h2>
            <p className="mt-2 text-sm text-gray-600 dark:text-gray-400">
              Your account role requires two-factor authentication to be enabled.
              Please set up 2FA before you can continue.
            </p>
          </div>

          <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-200 dark:border-gray-700 p-6">
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
              After setting up 2FA, you'll be able to sign in. You'll need an authenticator app like:
            </p>
            <ul className="text-sm text-gray-600 dark:text-gray-400 space-y-1 mb-6 ml-4 list-disc">
              <li>Google Authenticator</li>
              <li>Authy</li>
              <li>1Password</li>
              <li>Microsoft Authenticator</li>
            </ul>
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
              Please contact your administrator to set up 2FA for your account, or if you have access,
              sign in through another method and visit Account Settings.
            </p>
          </div>

          <button
            onClick={handleBack}
            className="w-full flex items-center justify-center gap-2 py-2 px-4 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 rounded-md hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
          >
            <ArrowLeft size={16} />
            Back to Login
          </button>
        </div>
      </div>
    )
  }

  // 2FA Code Entry View
  if (step === '2fa_required') {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center px-4">
        <div className="max-w-md w-full space-y-8">
          <div className="text-center">
            <div className="flex justify-center mb-4">
              <div className="p-3 bg-blue-100 dark:bg-blue-900/30 rounded-full">
                <Shield size={32} className="text-blue-600 dark:text-blue-400" />
              </div>
            </div>
            <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
              Two-Factor Authentication
            </h2>
            <p className="mt-2 text-sm text-gray-600 dark:text-gray-400">
              Enter the 6-digit code from your authenticator app
            </p>
          </div>

          <form className="mt-8 space-y-6" onSubmit={handleTotpSubmit}>
            {error && (
              <div className="rounded-md bg-red-100 dark:bg-red-900/50 border border-red-300 dark:border-red-700 p-4">
                <div className="text-sm text-red-700 dark:text-red-200">{error}</div>
              </div>
            )}

            <div className="flex justify-center">
              <input
                ref={totpInputRef}
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                autoComplete="one-time-code"
                value={totpCode}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
                placeholder="000000"
                maxLength={6}
                className="w-48 px-4 py-3 text-center text-2xl font-mono tracking-[0.5em] border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              />
            </div>

            <div className="space-y-3">
              <button
                type="submit"
                disabled={loading || totpCode.length !== 6}
                className="group relative w-full flex justify-center items-center gap-2 py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {loading && <Loader2 size={16} className="animate-spin" />}
                {loading ? 'Verifying...' : 'Verify'}
              </button>

              <button
                type="button"
                onClick={handleBack}
                className="w-full flex items-center justify-center gap-2 py-2 px-4 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 rounded-md hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
              >
                <ArrowLeft size={16} />
                Back to Login
              </button>
            </div>
          </form>
        </div>
      </div>
    )
  }

  // Default: Credentials Entry View
  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center px-4">
      <div className="max-w-md w-full space-y-8">
        <div className="text-center">
          <div className="flex justify-center mb-4">
            <Ship size={48} className="text-blue-500 dark:text-blue-400" strokeWidth={2} />
          </div>
          <h2 className="text-3xl font-extrabold text-gray-900 dark:text-white">
            Kubarr Dashboard
          </h2>
          <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
            Sign in to your account
          </p>
        </div>

        <form className="mt-8 space-y-6" onSubmit={handleCredentialsSubmit}>
          {error && (
            <div className="rounded-md bg-red-100 dark:bg-red-900/50 border border-red-300 dark:border-red-700 p-4">
              <div className="text-sm text-red-700 dark:text-red-200">{error}</div>
            </div>
          )}

          <div className="rounded-md shadow-sm -space-y-px">
            <div>
              <label htmlFor="username" className="sr-only">
                Username
              </label>
              <input
                id="username"
                name="username"
                type="text"
                required
                autoFocus
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="appearance-none rounded-none relative block w-full px-3 py-2 border border-gray-300 dark:border-gray-700 placeholder-gray-500 text-gray-900 dark:text-white bg-white dark:bg-gray-800 rounded-t-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                placeholder="Username"
              />
            </div>
            <div>
              <label htmlFor="password" className="sr-only">
                Password
              </label>
              <input
                id="password"
                name="password"
                type="password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="appearance-none rounded-none relative block w-full px-3 py-2 border border-gray-300 dark:border-gray-700 placeholder-gray-500 text-gray-900 dark:text-white bg-white dark:bg-gray-800 rounded-b-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 focus:z-10 sm:text-sm"
                placeholder="Password"
              />
            </div>
          </div>

          <div>
            <button
              type="submit"
              disabled={loading}
              className="group relative w-full flex justify-center items-center gap-2 py-2 px-4 border border-transparent text-sm font-medium rounded-md text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading && <Loader2 size={16} className="animate-spin" />}
              {loading ? 'Signing in...' : 'Sign in'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
