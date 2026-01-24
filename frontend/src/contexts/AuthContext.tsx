import React, { createContext, useContext, useState, useEffect, ReactNode, useMemo } from 'react';
import { getCurrentUser, User } from '../api/users';

interface AuthContextType {
  user: User | null;
  loading: boolean;
  isAuthenticated: boolean;
  isAdmin: boolean;
  allowedApps: Set<string> | null;
  canAccessApp: (appName: string) => boolean;
  checkAuth: () => Promise<void>;
  logout: () => void;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export const useAuth = () => {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
};

interface AuthProviderProps {
  children: ReactNode;
}

export const AuthProvider: React.FC<AuthProviderProps> = ({ children }) => {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  const checkAuth = async () => {
    try {
      setLoading(true);
      const currentUser = await getCurrentUser();
      setUser(currentUser);
    } catch (error) {
      setUser(null);
      // If we get a 401, the user is not authenticated
      // OAuth2-Proxy should redirect to login automatically
      console.error('Authentication check failed:', error);
    } finally {
      setLoading(false);
    }
  };

  const logout = () => {
    setUser(null);
    // Clear token from localStorage
    localStorage.removeItem('access_token');
    // Redirect to oauth2-proxy sign_out to clear session and trigger re-auth
    window.location.href = '/oauth2/sign_out';
  };

  useEffect(() => {
    checkAuth();
  }, []);

  // Compute allowed apps based on user roles
  const isAdmin = user?.is_admin || user?.roles?.some(r => r.name === 'admin') || false;

  const allowedApps = useMemo(() => {
    if (!user?.roles?.length) return new Set<string>();
    if (isAdmin) return null; // null means all apps

    const apps = new Set<string>();
    // Note: We don't have app_names in RoleInfo, so filtering will be done by the backend
    // This is a placeholder for frontend-side filtering if needed
    return apps;
  }, [user, isAdmin]);

  const canAccessApp = (_appName: string): boolean => {
    if (isAdmin) return true;
    if (allowedApps === null) return true;
    // For now, trust backend filtering - return true and let API calls handle permissions
    return true;
  };

  const value: AuthContextType = {
    user,
    loading,
    isAuthenticated: user !== null,
    isAdmin,
    allowedApps,
    canAccessApp,
    checkAuth,
    logout,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};
