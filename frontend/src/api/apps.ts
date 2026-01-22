import apiClient from './client';
import type { AppConfig, DeploymentRequest, DeploymentStatus } from '../types';

export const appsApi = {
  // Get all apps in catalog
  getCatalog: async (): Promise<AppConfig[]> => {
    const response = await apiClient.get<AppConfig[]>('/apps/catalog');
    return response.data;
  },

  // Get specific app from catalog
  getApp: async (appName: string): Promise<AppConfig> => {
    const response = await apiClient.get<AppConfig>(`/apps/catalog/${appName}`);
    return response.data;
  },

  // Get installed apps
  getInstalled: async (namespace: string = 'media'): Promise<string[]> => {
    const response = await apiClient.get<string[]>('/apps/installed', {
      params: { namespace },
    });
    return response.data;
  },

  // Install app
  install: async (request: DeploymentRequest): Promise<DeploymentStatus> => {
    const response = await apiClient.post<DeploymentStatus>('/apps/install', request);
    return response.data;
  },

  // Delete app
  delete: async (appName: string, namespace: string = 'media'): Promise<void> => {
    await apiClient.delete(`/apps/${appName}`, {
      params: { namespace },
    });
  },

  // Restart app
  restart: async (appName: string, namespace: string = 'media'): Promise<void> => {
    await apiClient.post(`/apps/${appName}/restart`, null, {
      params: { namespace },
    });
  },

  // Get categories
  getCategories: async (): Promise<string[]> => {
    const response = await apiClient.get<string[]>('/apps/categories');
    return response.data;
  },

  // Get apps by category
  getByCategory: async (category: string): Promise<AppConfig[]> => {
    const response = await apiClient.get<AppConfig[]>(`/apps/category/${category}`);
    return response.data;
  },
};
