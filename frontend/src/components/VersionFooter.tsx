import { useState, useEffect } from 'react';

interface BackendVersion {
  commit_hash: string;
  build_time: string;
}

export function VersionFooter() {
  const [backendVersion, setBackendVersion] = useState<BackendVersion | null>(null);

  const frontendCommit = __COMMIT_HASH__;
  const frontendBuildTime = __BUILD_TIME__;

  useEffect(() => {
    fetch('/api/system/version')
      .then(res => res.json())
      .then(data => setBackendVersion(data))
      .catch(() => setBackendVersion({ commit_hash: 'error', build_time: '' }));
  }, []);

  return (
    <footer className="fixed bottom-0 left-0 right-0 bg-gray-900 border-t border-gray-700 px-4 py-2 text-xs text-gray-500">
      <div className="flex justify-center gap-6">
        <span>
          Frontend: <code className="text-gray-400">{frontendCommit}</code>
          <span className="text-gray-600 ml-1">({new Date(frontendBuildTime).toLocaleString()})</span>
        </span>
        <span>|</span>
        <span>
          Backend: <code className="text-gray-400">{backendVersion?.commit_hash || 'loading...'}</code>
          {backendVersion?.build_time && (
            <span className="text-gray-600 ml-1">({new Date(backendVersion.build_time).toLocaleString()})</span>
          )}
        </span>
      </div>
    </footer>
  );
}
