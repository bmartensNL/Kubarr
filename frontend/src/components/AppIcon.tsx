import { useState } from 'react';

interface AppIconProps {
  appName: string;
  size?: number;
  className?: string;
}

export function AppIcon({ appName, size = 40, className = '' }: AppIconProps) {
  const [hasError, setHasError] = useState(false);

  // Build the icon URL from the backend API
  const iconUrl = `/api/apps/catalog/${appName.toLowerCase()}/icon`;

  if (hasError) {
    // Fallback to first letter if icon fails to load
    return (
      <div
        className={`flex items-center justify-center bg-gray-600 rounded-lg text-white font-bold ${className}`}
        style={{ width: size, height: size, fontSize: size * 0.5 }}
      >
        {appName.charAt(0).toUpperCase()}
      </div>
    );
  }

  return (
    <img
      src={iconUrl}
      alt={`${appName} icon`}
      width={size}
      height={size}
      className={`rounded-lg ${className}`}
      onError={() => setHasError(true)}
    />
  );
}

export default AppIcon;
