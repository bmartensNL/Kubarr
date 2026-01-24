import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { setupApi, SetupRequest } from '../api/setup';

type SetupStep = 'storage' | 'admin' | 'summary';

const SetupPage: React.FC = () => {
  const navigate = useNavigate();
  const [currentStep, setCurrentStep] = useState<SetupStep>('storage');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [validatingPath, setValidatingPath] = useState(false);
  const [pathValid, setPathValid] = useState<boolean | null>(null);
  const [pathError, setPathError] = useState<string | null>(null);

  // Form data
  const [storagePath, setStoragePath] = useState('/mnt/data/kubarr');
  const [adminUsername, setAdminUsername] = useState('admin');
  const [adminEmail, setAdminEmail] = useState('');
  const [adminPassword, setAdminPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');

  // Check if setup is already complete
  useEffect(() => {
    const checkSetup = async () => {
      try {
        const { setup_required } = await setupApi.checkRequired();
        if (!setup_required) {
          navigate('/login');
        }
      } catch (err) {
        // If we can't check, let the user try setup
      }
    };
    checkSetup();
  }, [navigate]);

  const validateStoragePath = async () => {
    if (!storagePath.trim()) {
      setPathValid(false);
      setPathError('Storage path is required');
      return false;
    }

    setValidatingPath(true);
    setPathError(null);
    setPathValid(null);

    try {
      const result = await setupApi.validatePath(storagePath);
      setPathValid(result.valid);
      setPathError(result.error);
      return result.valid;
    } catch (err) {
      setPathValid(false);
      setPathError('Failed to validate path');
      return false;
    } finally {
      setValidatingPath(false);
    }
  };

  const handleStorageNext = async () => {
    const valid = await validateStoragePath();
    if (valid) {
      setCurrentStep('admin');
    }
  };

  const validateAdminForm = (): boolean => {
    if (!adminUsername.trim()) {
      setError('Username is required');
      return false;
    }
    if (!adminEmail.trim() || !adminEmail.includes('@')) {
      setError('Valid email is required');
      return false;
    }
    if (!adminPassword || adminPassword.length < 8) {
      setError('Password must be at least 8 characters');
      return false;
    }
    if (adminPassword !== confirmPassword) {
      setError('Passwords do not match');
      return false;
    }
    setError('');
    return true;
  };

  const handleAdminNext = () => {
    if (validateAdminForm()) {
      setCurrentStep('summary');
    }
  };

  const handleSubmit = async () => {
    setError('');
    setLoading(true);

    try {
      const setupData: SetupRequest = {
        admin_username: adminUsername,
        admin_email: adminEmail,
        admin_password: adminPassword,
        storage_path: storagePath,
        base_url: window.location.origin,
      };

      const result = await setupApi.initialize(setupData);

      if (result.success) {
        // Force full page reload to re-check setup status
        window.location.href = '/login';
      } else {
        setError(result.message || 'Setup failed');
      }
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Setup failed. Please try again.');
    } finally {
      setLoading(false);
    }
  };

  const steps = [
    { id: 'storage', label: 'Storage', number: 1 },
    { id: 'admin', label: 'Admin User', number: 2 },
    { id: 'summary', label: 'Summary', number: 3 },
  ];

  const currentStepIndex = steps.findIndex(s => s.id === currentStep);

  return (
    <div className="min-h-screen bg-gray-900 flex items-center justify-center px-4 py-12">
      <div className="max-w-2xl w-full space-y-8">
        {/* Header */}
        <div className="text-center">
          <h1 className="text-3xl font-extrabold text-white">
            Welcome to Kubarr
          </h1>
          <p className="mt-2 text-gray-400">
            Let's set up your media management dashboard
          </p>
        </div>

        {/* Progress Indicator */}
        <div className="flex justify-center items-center space-x-4">
          {steps.map((step, index) => (
            <React.Fragment key={step.id}>
              <div className="flex items-center">
                <div
                  className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium ${
                    index <= currentStepIndex
                      ? 'bg-blue-600 text-white'
                      : 'bg-gray-700 text-gray-400'
                  }`}
                >
                  {step.number}
                </div>
                <span
                  className={`ml-2 text-sm ${
                    index <= currentStepIndex ? 'text-white' : 'text-gray-500'
                  }`}
                >
                  {step.label}
                </span>
              </div>
              {index < steps.length - 1 && (
                <div
                  className={`w-16 h-0.5 ${
                    index < currentStepIndex ? 'bg-blue-600' : 'bg-gray-700'
                  }`}
                />
              )}
            </React.Fragment>
          ))}
        </div>

        {/* Form Container */}
        <div className="bg-gray-800 rounded-lg shadow-xl p-8">
          {error && (
            <div className="mb-6 rounded-md bg-red-900 p-4">
              <div className="text-sm text-red-200">{error}</div>
            </div>
          )}

          {/* Step 1: Storage Configuration */}
          {currentStep === 'storage' && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-semibold text-white mb-2">
                  Storage Configuration
                </h2>
                <p className="text-gray-400 text-sm">
                  Specify the root path where all media files and downloads will be stored.
                  This path must exist on the host machine and be writable.
                </p>
              </div>

              <div>
                <label htmlFor="storagePath" className="block text-sm font-medium text-gray-300">
                  Storage Path
                </label>
                <div className="mt-1 flex rounded-md shadow-sm">
                  <input
                    type="text"
                    id="storagePath"
                    value={storagePath}
                    onChange={(e) => {
                      setStoragePath(e.target.value);
                      setPathValid(null);
                      setPathError(null);
                    }}
                    className="flex-1 block w-full px-3 py-2 border border-gray-700 bg-gray-900 text-white rounded-l-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                    placeholder="/mnt/data/kubarr"
                  />
                  <button
                    type="button"
                    onClick={validateStoragePath}
                    disabled={validatingPath}
                    className="inline-flex items-center px-4 py-2 border border-l-0 border-gray-700 bg-gray-700 text-sm font-medium text-gray-300 hover:bg-gray-600 rounded-r-md focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
                  >
                    {validatingPath ? 'Checking...' : 'Validate'}
                  </button>
                </div>
                {pathValid === true && (
                  <p className="mt-2 text-sm text-green-400">
                    Path is valid and writable
                  </p>
                )}
                {pathValid === false && pathError && (
                  <p className="mt-2 text-sm text-red-400">{pathError}</p>
                )}
              </div>

              <div className="bg-gray-900 rounded-md p-4">
                <h3 className="text-sm font-medium text-gray-300 mb-2">
                  Folder Structure
                </h3>
                <p className="text-xs text-gray-500 mb-2">
                  The following folders will be created automatically:
                </p>
                <pre className="text-xs text-gray-400 font-mono">
{`${storagePath}/
├── downloads/
│   ├── qbittorrent/
│   ├── transmission/
│   ├── deluge/
│   ├── rutorrent/
│   ├── sabnzbd/
│   └── nzbget/
└── media/
    ├── movies/
    ├── tv/
    └── music/`}
                </pre>
              </div>

              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={handleStorageNext}
                  disabled={validatingPath}
                  className="px-6 py-2 bg-blue-600 text-white font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
                >
                  Next
                </button>
              </div>
            </div>
          )}

          {/* Step 2: Admin User */}
          {currentStep === 'admin' && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-semibold text-white mb-2">
                  Admin User
                </h2>
                <p className="text-gray-400 text-sm">
                  Create the initial administrator account.
                </p>
              </div>

              <div className="space-y-4">
                <div>
                  <label htmlFor="adminUsername" className="block text-sm font-medium text-gray-300">
                    Username
                  </label>
                  <input
                    type="text"
                    id="adminUsername"
                    value={adminUsername}
                    onChange={(e) => setAdminUsername(e.target.value)}
                    className="mt-1 block w-full px-3 py-2 border border-gray-700 bg-gray-900 text-white rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                  />
                </div>

                <div>
                  <label htmlFor="adminEmail" className="block text-sm font-medium text-gray-300">
                    Email
                  </label>
                  <input
                    type="email"
                    id="adminEmail"
                    value={adminEmail}
                    onChange={(e) => setAdminEmail(e.target.value)}
                    className="mt-1 block w-full px-3 py-2 border border-gray-700 bg-gray-900 text-white rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                    placeholder="admin@example.com"
                  />
                </div>

                <div>
                  <label htmlFor="adminPassword" className="block text-sm font-medium text-gray-300">
                    Password
                  </label>
                  <input
                    type="password"
                    id="adminPassword"
                    value={adminPassword}
                    onChange={(e) => setAdminPassword(e.target.value)}
                    className="mt-1 block w-full px-3 py-2 border border-gray-700 bg-gray-900 text-white rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                    placeholder="Minimum 8 characters"
                  />
                </div>

                <div>
                  <label htmlFor="confirmPassword" className="block text-sm font-medium text-gray-300">
                    Confirm Password
                  </label>
                  <input
                    type="password"
                    id="confirmPassword"
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    className="mt-1 block w-full px-3 py-2 border border-gray-700 bg-gray-900 text-white rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
                  />
                </div>
              </div>

              <div className="flex justify-between">
                <button
                  type="button"
                  onClick={() => setCurrentStep('storage')}
                  className="px-6 py-2 border border-gray-600 text-gray-300 font-medium rounded-md hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-500"
                >
                  Back
                </button>
                <button
                  type="button"
                  onClick={handleAdminNext}
                  className="px-6 py-2 bg-blue-600 text-white font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  Next
                </button>
              </div>
            </div>
          )}

          {/* Step 3: Summary */}
          {currentStep === 'summary' && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-semibold text-white mb-2">
                  Review & Confirm
                </h2>
                <p className="text-gray-400 text-sm">
                  Please review your configuration before completing setup.
                </p>
              </div>

              <div className="space-y-4">
                <div className="bg-gray-900 rounded-md p-4">
                  <h3 className="text-sm font-medium text-gray-300 mb-3">
                    Storage Configuration
                  </h3>
                  <dl className="space-y-2">
                    <div className="flex justify-between">
                      <dt className="text-sm text-gray-500">Root Path:</dt>
                      <dd className="text-sm text-white font-mono">{storagePath}</dd>
                    </div>
                  </dl>
                </div>

                <div className="bg-gray-900 rounded-md p-4">
                  <h3 className="text-sm font-medium text-gray-300 mb-3">
                    Admin User
                  </h3>
                  <dl className="space-y-2">
                    <div className="flex justify-between">
                      <dt className="text-sm text-gray-500">Username:</dt>
                      <dd className="text-sm text-white">{adminUsername}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-sm text-gray-500">Email:</dt>
                      <dd className="text-sm text-white">{adminEmail}</dd>
                    </div>
                  </dl>
                </div>
              </div>

              <div className="flex justify-between">
                <button
                  type="button"
                  onClick={() => setCurrentStep('admin')}
                  disabled={loading}
                  className="px-6 py-2 border border-gray-600 text-gray-300 font-medium rounded-md hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-500 disabled:opacity-50"
                >
                  Back
                </button>
                <button
                  type="button"
                  onClick={handleSubmit}
                  disabled={loading}
                  className="px-6 py-2 bg-green-600 text-white font-medium rounded-md hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 disabled:opacity-50"
                >
                  {loading ? 'Setting up...' : 'Complete Setup'}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default SetupPage;
