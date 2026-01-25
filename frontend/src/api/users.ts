import apiClient from './client';
import { RoleInfo } from './roles';

export interface User {
  id: number;
  username: string;
  email: string;
  is_active: boolean;
  is_admin: boolean;
  is_approved: boolean;
  created_at: string;
  updated_at: string;
  roles: RoleInfo[];
}

export interface CreateUserRequest {
  username: string;
  email: string;
  password: string;
  is_admin?: boolean;
  role_ids?: number[];
}

export interface UpdateUserRequest {
  email?: string;
  is_active?: boolean;
  is_admin?: boolean;
  is_approved?: boolean;
  role_ids?: number[];
}

/**
 * Get current authenticated user
 */
export const getCurrentUser = async (): Promise<User> => {
  const response = await apiClient.get<User>('/users/me');
  return response.data;
};

/**
 * Get all users (admin only)
 */
export const getUsers = async (): Promise<User[]> => {
  const response = await apiClient.get<User[]>('/users');
  return response.data;
};

/**
 * Get pending approval users (admin only)
 */
export const getPendingUsers = async (): Promise<User[]> => {
  const response = await apiClient.get<User[]>('/users/pending');
  return response.data;
};

/**
 * Get user by ID (admin only)
 */
export const getUser = async (userId: number): Promise<User> => {
  const response = await apiClient.get<User>(`/users/${userId}`);
  return response.data;
};

/**
 * Create a new user (admin only)
 */
export const createUser = async (userData: CreateUserRequest): Promise<User> => {
  const response = await apiClient.post<User>('/users', userData);
  return response.data;
};

/**
 * Update user (admin only)
 */
export const updateUser = async (userId: number, userData: UpdateUserRequest): Promise<User> => {
  const response = await apiClient.patch<User>(`/users/${userId}`, userData);
  return response.data;
};

/**
 * Approve user registration (admin only)
 */
export const approveUser = async (userId: number): Promise<{ message: string }> => {
  const response = await apiClient.post<{ message: string }>(`/users/${userId}/approve`);
  return response.data;
};

/**
 * Reject user registration (admin only)
 */
export const rejectUser = async (userId: number): Promise<{ message: string }> => {
  const response = await apiClient.post<{ message: string }>(`/users/${userId}/reject`);
  return response.data;
};

/**
 * Delete user (admin only)
 */
export const deleteUser = async (userId: number): Promise<{ message: string }> => {
  const response = await apiClient.delete<{ message: string }>(`/users/${userId}`);
  return response.data;
};

// Invite types and API functions
export interface Invite {
  id: number;
  code: string;
  created_by_username: string;
  used_by_username: string | null;
  is_used: boolean;
  expires_at: string | null;
  created_at: string;
  used_at: string | null;
}

export interface CreateInviteRequest {
  expires_in_days?: number;
}

/**
 * Get all invites (admin only)
 */
export const getInvites = async (): Promise<Invite[]> => {
  const response = await apiClient.get<Invite[]>('/users/invites');
  return response.data;
};

/**
 * Create a new invite (admin only)
 */
export const createInvite = async (data?: CreateInviteRequest): Promise<Invite> => {
  const response = await apiClient.post<Invite>('/users/invites', data || {});
  return response.data;
};

/**
 * Delete an invite (admin only)
 */
export const deleteInvite = async (inviteId: number): Promise<{ message: string }> => {
  const response = await apiClient.delete<{ message: string }>(`/users/invites/${inviteId}`);
  return response.data;
};
