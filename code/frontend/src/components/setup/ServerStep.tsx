import React, { useState, useEffect } from 'react';
import { Check, AlertCircle, Loader2 } from 'lucide-react';
import { setupApi, ServerConfigResponse } from '../../api/setup';

interface ServerStepProps {
  onComplete: (config: { name: string; storagePath: string }) => void;
  onBack: () => void;
  initialConfig?: ServerConfigResponse | null;
}

const ServerStep: React.FC<ServerStepProps> = ({ onComplete, onBack, initialConfig }) => {
  const [serverName, setServerName] = useState(initialConfig?.name || 'Kubarr');
  const [storagePath, setStoragePath] = useState(initialConfig?.storage_path || '/mnt/data/kubarr');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [validatingPath, setValidatingPath] = useState(false);
  const [pathValid, setPathValid] = useState<boolean | null>(null);
  const [pathMessage, setPathMessage] = useState<string | null>(null);

  // Load existing config on mount
  useEffect(() => {
    const loadConfig = async () => {
      try {
        const config = await setupApi.getServerConfig();
        if (config) {
          setServerName(config.name);
          setStoragePath(config.storage_path);
          setPathValid(true);
        }
      } catch (err) {
        // Config doesn't exist yet, use defaults
      }
    };
    if (!initialConfig) {
      loadConfig();
    }
  }, [initialConfig]);

  const validateStoragePath = async (): Promise<boolean> => {
    if (!storagePath.trim()) {
      setPathValid(false);
      setPathMessage('Storage path is required');
      return false;
    }

    setValidatingPath(true);
    setPathMessage(null);
    setPathValid(null);

    try {
      const result = await setupApi.validatePath(storagePath);
      setPathValid(result.valid);
      setPathMessage(result.message);
      return result.valid;
    } catch (err) {
      setPathValid(false);
      setPathMessage('Failed to validate path');
      return false;
    } finally {
      setValidatingPath(false);
    }
  };

  const handleSubmit = async () => {
    setError(null);

    // Validate server name
    if (!serverName.trim()) {
      setError('Server name is required');
      return;
    }

    // Validate storage path
    const pathIsValid = await validateStoragePath();
    if (!pathIsValid) {
      return;
    }

    setLoading(true);

    try {
      await setupApi.configureServer({
        name: serverName.trim(),
        storage_path: storagePath.trim(),
      });

      onComplete({
        name: serverName.trim(),
        storagePath: storagePath.trim(),
      });
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Failed to save server configuration');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-gray-900 dark:text-white mb-2">
          Server Configuration
        </h2>
        <p className="text-gray-500 dark:text-gray-400 text-sm">
          Configure your server name and storage location for media files.
        </p>
      </div>

      {error && (
        <div className="rounded-md bg-red-900/50 border border-red-700 p-4">
          <div className="flex items-center space-x-2">
            <AlertCircle className="w-5 h-5 text-red-400" />
            <span className="text-sm text-red-200">{error}</span>
          </div>
        </div>
      )}

      <div className="space-y-4">
        {/* Server Name */}
        <div>
          <label htmlFor="serverName" className="block text-sm font-medium text-gray-700 dark:text-gray-300">
            Server Name
          </label>
          <input
            type="text"
            id="serverName"
            value={serverName}
            onChange={(e) => setServerName(e.target.value)}
            className="mt-1 block w-full px-3 py-2 border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-900 dark:text-white rounded-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
            placeholder="My Kubarr Server"
          />
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            A friendly name for your Kubarr instance
          </p>
        </div>

        {/* Storage Path */}
        <div>
          <label htmlFor="storagePath" className="block text-sm font-medium text-gray-700 dark:text-gray-300">
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
                setPathMessage(null);
              }}
              onBlur={validateStoragePath}
              className="flex-1 block w-full px-3 py-2 border border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 text-gray-900 dark:text-white rounded-l-md focus:outline-none focus:ring-blue-500 focus:border-blue-500 sm:text-sm"
              placeholder="/mnt/data/kubarr"
            />
            <button
              type="button"
              onClick={validateStoragePath}
              disabled={validatingPath}
              className="inline-flex items-center px-4 py-2 border border-l-0 border-gray-300 dark:border-gray-700 bg-gray-200 dark:bg-gray-700 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-300 dark:hover:bg-gray-600 rounded-r-md focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
            >
              {validatingPath ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Validate'}
            </button>
          </div>
          {pathValid === true && (
            <p className="mt-2 text-sm text-green-400 flex items-center space-x-1">
              <Check className="w-4 h-4" />
              <span>{pathMessage || 'Path is valid'}</span>
            </p>
          )}
          {pathValid === false && pathMessage && (
            <p className="mt-2 text-sm text-red-400 flex items-center space-x-1">
              <AlertCircle className="w-4 h-4" />
              <span>{pathMessage}</span>
            </p>
          )}
        </div>

        {/* Folder structure preview */}
        <div className="bg-gray-100 dark:bg-gray-900 rounded-md p-4">
          <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
            Folder Structure
          </h3>
          <p className="text-xs text-gray-500 mb-2">
            The following folders will be created automatically:
          </p>
          <pre className="text-xs text-gray-500 dark:text-gray-400 font-mono overflow-x-auto">
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
      </div>

      <div className="flex justify-between">
        <button
          type="button"
          onClick={onBack}
          disabled={loading}
          className="px-6 py-2 border border-gray-600 text-gray-300 font-medium rounded-md hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-500 disabled:opacity-50"
        >
          Back
        </button>
        <button
          type="button"
          onClick={handleSubmit}
          disabled={loading || validatingPath}
          className="px-6 py-2 bg-blue-600 text-white font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
        >
          {loading ? (
            <span className="flex items-center space-x-2">
              <Loader2 className="w-4 h-4 animate-spin" />
              <span>Saving...</span>
            </span>
          ) : (
            'Next'
          )}
        </button>
      </div>
    </div>
  );
};

export default ServerStep;
