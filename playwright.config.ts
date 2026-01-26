import { defineConfig, devices } from '@playwright/test';

/**
 * Kubarr E2E Test Configuration
 *
 * Tests run against the deployed application via port-forward:
 * kubectl port-forward svc/oauth2-proxy 8080:80 -n oauth2-proxy
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false, // Run tests sequentially for auth state management
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1, // Single worker for consistent auth state
  reporter: [
    ['html', { open: 'never' }],
    ['list']
  ],

  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:8080',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Global timeout for each test
  timeout: 60000,

  // Expect timeout
  expect: {
    timeout: 10000,
  },
});
