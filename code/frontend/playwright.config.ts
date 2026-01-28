import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for Kubarr UI tests
 *
 * Before running tests, ensure:
 * 1. Backend is running: kubectl port-forward -n kubarr svc/kubarr-backend 8080:8000
 * 2. Tests will start the Vite dev server automatically
 */
export default defineConfig({
  testDir: './tests',
  /* Run tests in files in parallel */
  fullyParallel: true,
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,
  /* Opt out of parallel tests on CI. */
  workers: process.env.CI ? 1 : undefined,
  /* Reporter to use */
  reporter: [
    ['html'],
    ['list']
  ],
  /* Shared settings for all the projects below */
  use: {
    /* Base URL for the Vite dev server */
    baseURL: 'http://localhost:5173',

    /* Collect trace when retrying the failed test */
    trace: 'on-first-retry',

    /* Screenshot on failure */
    screenshot: 'only-on-failure',

    /* Video on failure */
    video: 'retain-on-failure',

    /* Slow down actions for demo (set via SLOW_MO env var) */
    launchOptions: {
      slowMo: process.env.SLOW_MO ? parseInt(process.env.SLOW_MO) : 0,
    },

    /* Large viewport for better visibility */
    viewport: { width: 1920, height: 1080 },
  },

  /* Configure projects for major browsers */
  projects: [
    // Setup project - authenticates and saves state
    {
      name: 'setup',
      testMatch: /.*\.setup\.ts/,
    },

    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        // Use authenticated state from setup
        storageState: 'tests/.auth/user.json',
        // Use large viewport for visibility
        viewport: { width: 1920, height: 1080 },
      },
      dependencies: ['setup'],
    },
  ],

  /* Timeout settings */
  timeout: 30000,
  expect: {
    timeout: 5000,
  },

  /* Start Vite dev server before tests */
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
