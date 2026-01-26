import axios from 'axios';

// Auth API uses a separate client without /api prefix since auth routes are at /auth/*
const authClient = axios.create({
  baseURL: '/auth',
  withCredentials: true,
});

// ============================================================================
// Types
// ============================================================================

export type SessionLoginStatus = 'success' | '2fa_required' | '2fa_setup_required';

export interface SessionLoginResponse {
  status: SessionLoginStatus;
  challenge_token?: string;
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface Verify2FARequest {
  challenge_token: string;
  code: string;
}

// ============================================================================
// Session Auth API
// ============================================================================

/**
 * Login with username and password
 * Returns status indicating if 2FA is required
 */
export const sessionLogin = async (credentials: LoginRequest): Promise<SessionLoginResponse> => {
  const response = await authClient.post<SessionLoginResponse>('/session/login', credentials);
  return response.data;
};

/**
 * Verify 2FA code during login
 */
export const verify2FA = async (data: Verify2FARequest): Promise<SessionLoginResponse> => {
  const response = await authClient.post<SessionLoginResponse>('/session/2fa/verify', data);
  return response.data;
};

/**
 * Logout (clears session cookie)
 */
export const sessionLogout = async (): Promise<{ success: boolean }> => {
  const response = await authClient.post<{ success: boolean }>('/session/logout');
  return response.data;
};

/**
 * Verify current session is valid
 */
export const verifySession = async (): Promise<boolean> => {
  try {
    await authClient.get('/session/verify');
    return true;
  } catch {
    return false;
  }
};
