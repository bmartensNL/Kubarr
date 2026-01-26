import React, { createContext, useContext, useState, useEffect, ReactNode, useMemo } from 'react';
import { getCurrentUser, User } from '../api/users';

interface AuthContextType {
  user: User | null;
  loading: boolean;
  isAuthenticated: boolean;
  isAdmin: boolean;
  permissions: Set<string>;
  allowedApps: Set<string> | null;
  hasPermission: (permission: string) => boolean;
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
  };

  useEffect(() => {
    // Skip auth check on login page to prevent redirect loop
    if (window.location.pathname === '/login') {
      setLoading(false);
      return;
    }
    checkAuth();
  }, []);

  // Check if user has admin role
  const isAdmin = user?.roles?.some(r => r.name === 'admin') || false;

  // Get user permissions from the backend response
  const permissions = useMemo(() => {
    if (!user?.permissions) return new Set<string>();
    return new Set(user.permissions);
  }, [user]);

  const allowedApps = useMemo(() => {
    if (!user) return new Set<string>();
    if (isAdmin) return null; // null means all apps
    return new Set(user.allowed_apps || []);
  }, [user, isAdmin]);

  const hasPermission = (permission: string): boolean => {
    if (isAdmin) return true;
    return permissions.has(permission);
  };

  const canAccessApp = (appName: string): boolean => {
    if (isAdmin) return true;
    if (allowedApps === null) return true;
    return allowedApps.has(appName);
  };

  const value: AuthContextType = {
    user,
    loading,
    isAuthenticated: user !== null,
    isAdmin,
    permissions,
    allowedApps,
    hasPermission,
    canAccessApp,
    checkAuth,
    logout,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};
