import qbittorrentIcon from '../assets/icons/qbittorrent.svg';
import jackettIcon from '../assets/icons/jackett.svg';
import sonarrIcon from '../assets/icons/sonarr.svg';
import radarrIcon from '../assets/icons/radarr.svg';
import jellyfinIcon from '../assets/icons/jellyfin.svg';
import jellyseerrIcon from '../assets/icons/jellyseerr.svg';
import sabnzbdIcon from '../assets/icons/sabnzbd.svg';
import transmissionIcon from '../assets/icons/transmission.svg';
import delugeIcon from '../assets/icons/deluge.svg';
import rutorrentIcon from '../assets/icons/rutorrent.svg';

const icons: Record<string, string> = {
  qbittorrent: qbittorrentIcon,
  jackett: jackettIcon,
  sonarr: sonarrIcon,
  radarr: radarrIcon,
  jellyfin: jellyfinIcon,
  jellyseerr: jellyseerrIcon,
  sabnzbd: sabnzbdIcon,
  transmission: transmissionIcon,
  deluge: delugeIcon,
  rutorrent: rutorrentIcon,
};

interface AppIconProps {
  appName: string;
  size?: number;
  className?: string;
}

export function AppIcon({ appName, size = 40, className = '' }: AppIconProps) {
  const icon = icons[appName.toLowerCase()];

  if (!icon) {
    // Fallback to a generic icon or first letter
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
      src={icon}
      alt={`${appName} icon`}
      width={size}
      height={size}
      className={`rounded-lg ${className}`}
    />
  );
}

export default AppIcon;
