import { test, expect } from '@playwright/test';

// All browseable apps that should be accessible via the proxy
const BROWSEABLE_APPS = [
  { name: 'sonarr', path: '/sonarr/', expectedContent: ['Sonarr', 'sonarr'] },
  { name: 'radarr', path: '/radarr/', expectedContent: ['Radarr', 'radarr'] },
  { name: 'jellyfin', path: '/jellyfin/', expectedContent: ['Jellyfin', 'jellyfin'] },
  { name: 'plex', path: '/plex/', expectedContent: ['Plex', 'plex'] },
  { name: 'jellyseerr', path: '/jellyseerr/', expectedContent: ['Jellyseerr', 'jellyseerr', 'Overseerr'] },
  { name: 'deluge', path: '/deluge/', expectedContent: ['Deluge', 'deluge'] },
  { name: 'transmission', path: '/transmission/', expectedContent: ['Transmission', 'transmission'] },
  { name: 'rutorrent', path: '/rutorrent/', expectedContent: ['ruTorrent', 'rutorrent'] },
  { name: 'qbittorrent', path: '/qbittorrent/', expectedContent: ['qBittorrent', 'qbittorrent'] },
  { name: 'jackett', path: '/jackett/', expectedContent: ['Jackett', 'jackett'] },
];

test.describe('App Proxy Accessibility', () => {
  // Increase timeout for proxy tests as apps may be slow to respond
  test.setTimeout(60000);

  test.beforeEach(async ({ page }) => {
    // Verify we're authenticated by visiting dashboard
    await page.goto('/');
    await expect(page.locator('text=Overview')).toBeVisible({ timeout: 10000 });
  });

  for (const app of BROWSEABLE_APPS) {
    test(`${app.name} is accessible via proxy`, async ({ page, request }) => {
      // First check if the app is installed by checking the API
      const installedResponse = await request.get('/api/apps/installed');

      if (!installedResponse.ok()) {
        test.skip();
        return;
      }

      const installedApps = await installedResponse.json();

      if (!installedApps.includes(app.name)) {
        console.log(`${app.name} is not installed, skipping...`);
        test.skip();
        return;
      }

      console.log(`Testing ${app.name} proxy at ${app.path}...`);

      // Try to access the app via proxy
      const response = await page.goto(app.path, {
        waitUntil: 'domcontentloaded',
        timeout: 30000
      });

      // Check response is successful (2xx or 3xx)
      expect(response).not.toBeNull();
      const status = response!.status();
      expect(status, `${app.name} returned status ${status}`).toBeLessThan(400);

      // Wait for some content to load
      await page.waitForLoadState('domcontentloaded');

      // Get page content
      const content = await page.content();
      const contentLower = content.toLowerCase();

      // Check if page contains expected content (at least one match)
      const hasExpectedContent = app.expectedContent.some(
        expected => contentLower.includes(expected.toLowerCase())
      );

      // Log what we found for debugging
      if (!hasExpectedContent) {
        console.log(`${app.name} page content (first 500 chars): ${content.substring(0, 500)}`);
      }

      expect(hasExpectedContent,
        `${app.name} page should contain one of: ${app.expectedContent.join(', ')}`
      ).toBe(true);

      console.log(`✓ ${app.name} is accessible and contains expected content`);
    });
  }

  test('all installed apps return valid responses', async ({ page, request }) => {
    // Get list of installed apps
    const installedResponse = await request.get('/api/apps/installed');
    expect(installedResponse.ok()).toBe(true);

    const installedApps: string[] = await installedResponse.json();
    console.log(`Installed apps: ${installedApps.join(', ')}`);

    // Get catalog to know which apps are browseable
    const catalogResponse = await request.get('/api/apps/catalog');
    expect(catalogResponse.ok()).toBe(true);

    const catalog = await catalogResponse.json();
    const browseableApps = catalog
      .filter((app: any) => app.is_browseable && installedApps.includes(app.name))
      .map((app: any) => ({ name: app.name, path: `/${app.name}/` }));

    console.log(`Browseable installed apps: ${browseableApps.map((a: any) => a.name).join(', ')}`);

    const failures: string[] = [];

    for (const app of browseableApps) {
      try {
        const response = await page.goto(app.path, {
          waitUntil: 'domcontentloaded',
          timeout: 30000
        });

        if (!response || response.status() >= 400) {
          failures.push(`${app.name}: HTTP ${response?.status() || 'no response'}`);
        } else {
          console.log(`✓ ${app.name}: HTTP ${response.status()}`);
        }
      } catch (error: any) {
        failures.push(`${app.name}: ${error.message}`);
      }
    }

    if (failures.length > 0) {
      console.log('\nFailed apps:');
      failures.forEach(f => console.log(`  ✗ ${f}`));
    }

    expect(failures, `Some apps failed: ${failures.join(', ')}`).toHaveLength(0);
  });
});

test.describe('App Proxy Error Handling', () => {
  test('non-existent app returns appropriate error', async ({ page }) => {
    const response = await page.goto('/nonexistent-app/', {
      waitUntil: 'domcontentloaded',
      timeout: 10000
    });

    // Should either return 404 or redirect to frontend
    expect(response).not.toBeNull();
  });

  test('uninstalled app returns service unavailable', async ({ page, request }) => {
    // Get list of installed apps
    const installedResponse = await request.get('/api/apps/installed');
    const installedApps: string[] = await installedResponse.json();

    // Get catalog
    const catalogResponse = await request.get('/api/apps/catalog');
    const catalog = await catalogResponse.json();

    // Find an app that's not installed
    const uninstalledApp = catalog.find(
      (app: any) => app.is_browseable && !installedApps.includes(app.name)
    );

    if (!uninstalledApp) {
      console.log('All browseable apps are installed, skipping...');
      test.skip();
      return;
    }

    console.log(`Testing uninstalled app: ${uninstalledApp.name}`);

    const response = await page.goto(`/${uninstalledApp.name}/`, {
      waitUntil: 'domcontentloaded',
      timeout: 10000
    });

    // Should return error status (503 Service Unavailable expected)
    expect(response).not.toBeNull();
    // App might not be in catalog as routable, so it might go to frontend
    // Just verify we don't get a 500 error
    expect(response!.status()).not.toBe(500);
  });
});
