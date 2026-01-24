import axios from 'axios';

// Separate client for setup endpoints (no auth required)
const setupClient = axios.create({
  baseURL: import.meta.env.VITE_API_URL || '/api',
  timeout: 30000, // Longer timeout for setup operations
  headers: {
    'Content-Type': 'application/json',
  },
});

export interface SetupStatusResponse {
  setup_required: boolean;
  admin_user_exists: boolean;
  oauth2_client_exists: boolean;
  storage_configured: boolean;
}

export interface SetupRequest {
  admin_username: string;
  admin_email: string;
  admin_password: string;
  storage_path: string;
  base_url?: string;
  oauth2_client_secret?: string;
}

export interface SetupResult {
  success: boolean;
  message: string;
  data: {
    admin_user: {
      id: number;
      username: string;
      email: string;
    };
    storage: {
      path: string;
      folders_created: string[];
    };
    oauth2_client?: {
      client_id: string;
      client_secret: string;
      redirect_uris: string[];
    };
  };
}

export interface GeneratedCredentials {
  admin_username: string;
  admin_email: string;
  admin_password: string;
  client_secret: string;
}

export interface PathValidationResult {
  valid: boolean;
  error: string | null;
  path: string;
}

export const setupApi = {
  // Check if setup is required (accessible without auth)
  checkRequired: async (): Promise<{ setup_required: boolean }> => {
    const response = await setupClient.get('/setup/required');
    return response.data;
  },

  // Get setup status (only accessible during setup)
  getStatus: async (): Promise<SetupStatusResponse> => {
    const response = await setupClient.get<SetupStatusResponse>('/setup/status');
    return response.data;
  },

  // Initialize the setup
  initialize: async (data: SetupRequest): Promise<SetupResult> => {
    const response = await setupClient.post<SetupResult>('/setup/initialize', data);
    return response.data;
  },

  // Generate random credentials
  generateCredentials: async (): Promise<GeneratedCredentials> => {
    const response = await setupClient.get<GeneratedCredentials>('/setup/generate-credentials');
    return response.data;
  },

  // Validate storage path
  validatePath: async (path: string): Promise<PathValidationResult> => {
    const response = await setupClient.post<PathValidationResult>('/setup/validate-path', null, {
      params: { path },
    });
    return response.data;
  },
};
