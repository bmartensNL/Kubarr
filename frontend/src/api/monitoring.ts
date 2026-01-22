import apiClient from './client';
import type { AppHealth, PodMetrics, PodStatus, ServiceEndpoint } from '../types';

export const monitoringApi = {
  // Get pod status
  getPodStatus: async (namespace: string = 'media', app?: string): Promise<PodStatus[]> => {
    const response = await apiClient.get<PodStatus[]>('/monitoring/pods', {
      params: { namespace, app },
    });
    return response.data;
  },

  // Get pod metrics
  getMetrics: async (namespace: string = 'media', app?: string): Promise<PodMetrics[]> => {
    const response = await apiClient.get<PodMetrics[]>('/monitoring/metrics', {
      params: { namespace, app },
    });
    return response.data;
  },

  // Get app health
  getAppHealth: async (appName: string, namespace: string = 'media'): Promise<AppHealth> => {
    const response = await apiClient.get<AppHealth>(`/monitoring/health/${appName}`, {
      params: { namespace },
    });
    return response.data;
  },

  // Get service endpoints
  getEndpoints: async (
    appName: string,
    namespace: string = 'media'
  ): Promise<ServiceEndpoint[]> => {
    const response = await apiClient.get<ServiceEndpoint[]>(
      `/monitoring/endpoints/${appName}`,
      {
        params: { namespace },
      }
    );
    return response.data;
  },

  // Check if metrics server is available
  checkMetricsAvailable: async (): Promise<boolean> => {
    const response = await apiClient.get<{ available: boolean }>('/monitoring/metrics-available');
    return response.data.available;
  },
};
