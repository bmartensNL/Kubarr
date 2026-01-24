import apiClient from './client';

export interface Role {
  id: number;
  name: string;
  description: string | null;
  is_system: boolean;
  created_at: string;
  app_names: string[];
}

export interface RoleInfo {
  id: number;
  name: string;
  description: string | null;
}

export interface CreateRoleRequest {
  name: string;
  description?: string;
  app_names?: string[];
}

export interface UpdateRoleRequest {
  name?: string;
  description?: string;
}

export interface SetRoleAppsRequest {
  app_names: string[];
}

/**
 * Get all roles
 */
export const getRoles = async (): Promise<Role[]> => {
  const response = await apiClient.get<Role[]>('/roles/');
  return response.data;
};

/**
 * Get role by ID
 */
export const getRole = async (roleId: number): Promise<Role> => {
  const response = await apiClient.get<Role>(`/roles/${roleId}`);
  return response.data;
};

/**
 * Create a new role (admin only)
 */
export const createRole = async (roleData: CreateRoleRequest): Promise<Role> => {
  const response = await apiClient.post<Role>('/roles/', roleData);
  return response.data;
};

/**
 * Update role (admin only)
 */
export const updateRole = async (roleId: number, roleData: UpdateRoleRequest): Promise<Role> => {
  const response = await apiClient.patch<Role>(`/roles/${roleId}`, roleData);
  return response.data;
};

/**
 * Delete role (admin only)
 */
export const deleteRole = async (roleId: number): Promise<{ message: string }> => {
  const response = await apiClient.delete<{ message: string }>(`/roles/${roleId}`);
  return response.data;
};

/**
 * Set app permissions for a role (admin only)
 */
export const setRoleApps = async (roleId: number, apps: SetRoleAppsRequest): Promise<Role> => {
  const response = await apiClient.put<Role>(`/roles/${roleId}/apps`, apps);
  return response.data;
};
