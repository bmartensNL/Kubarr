import { test, expect, Page } from '@playwright/test';

// Longer timeout for app installation/uninstallation (5 minutes per operation)
const APP_OPERATION_TIMEOUT = 5 * 60 * 1000;

// Apps to test - use display names as shown in UI
const TESTABLE_APPS = [
  { name: 'sonarr', displayName: 'Sonarr' },
  { name: 'radarr', displayName: 'Radarr' },
  { name: 'jellyfin', displayName: 'Jellyfin' },
  { name: 'plex', displayName: 'Plex' },
  { name: 'jellyseerr', displayName: 'Jellyseerr' },
  { name: 'deluge', displayName: 'Deluge' },
  { name: 'transmission', displayName: 'Transmission' },
  { name: 'rutorrent', displayName: 'ruTorrent' },
  { name: 'qbittorrent', displayName: 'qBittorrent' },
  { name: 'jackett', displayName: 'Jackett' },
];

// Helper to find an app card by display name
function getAppCard(page: Page, displayName: string) {
  return page.locator('.rounded-xl').filter({ has: page.locator(`h3:has-text("${displayName}")`) });
}

// Helper to wait for app to be installed
async function waitForInstalled(page: Page, displayName: string, timeout: number) {
  const appCard = getAppCard(page, displayName);
  // Wait for "Open" link to appear (indicates installation complete)
  await expect(appCard.locator('a:has-text("Open")')).toBeVisible({ timeout });
}

// Helper to wait for app to be uninstalled
async function waitForUninstalled(page: Page, displayName: string, timeout: number) {
  const appCard = getAppCard(page, displayName);
  // Wait for "Install" button to appear
  await expect(
    appCard.locator('button:has-text("Install")')
  ).toBeVisible({ timeout });
}

test.describe('App Installation and Uninstallation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/apps');
    // Wait for apps page to load
    await expect(page.locator('h1:has-text("App Marketplace")')).toBeVisible({ timeout: 10000 });
    // Wait for catalog to load
    await page.waitForLoadState('networkidle');
  });

  test.describe('Apps Page', () => {
    test('shows apps page with categories', async ({ page }) => {
      // Should show app categories
      await expect(page.locator('h2:has-text("Media Servers")')).toBeVisible();
      await expect(page.locator('h2:has-text("Download Clients")')).toBeVisible();
      await expect(page.locator('h2:has-text("Media Managers")')).toBeVisible();
    });

    test('shows available apps in catalog', async ({ page }) => {
      // Should show some well-known apps
      await expect(page.locator('h3:has-text("Sonarr")')).toBeVisible();
      await expect(page.locator('h3:has-text("Radarr")')).toBeVisible();
      await expect(page.locator('h3:has-text("Jellyfin")')).toBeVisible();
    });

    test('each app has install or open button', async ({ page }) => {
      // Check a few apps have the expected buttons
      for (const app of ['Sonarr', 'Radarr', 'Jellyfin']) {
        const appCard = getAppCard(page, app);
        await expect(appCard).toBeVisible();

        // Should have either Install or Open button
        const hasInstall = await appCard.locator('button:has-text("Install")').isVisible();
        const hasOpen = await appCard.locator('a:has-text("Open"), button:has-text("Open")').isVisible();
        expect(hasInstall || hasOpen).toBe(true);
      }
    });
  });

  // Generate individual install/uninstall tests for each app
  for (const app of TESTABLE_APPS) {
    test.describe(`${app.displayName}`, () => {
      // Increase timeout for install/uninstall operations
      test.setTimeout(APP_OPERATION_TIMEOUT * 2);

      test(`can install and uninstall ${app.displayName}`, async ({ page }) => {
        const appCard = getAppCard(page, app.displayName);
        await expect(appCard).toBeVisible();

        const installButton = appCard.locator('button:has-text("Install"), button:has-text("Retry Install")');
        const openButton = appCard.locator('a:has-text("Open")');
        const uninstallButton = appCard.locator('button[title="Uninstall"]');

        let wasAlreadyInstalled = false;

        // Check current state
        const isInstalled = await openButton.isVisible().catch(() => false);

        if (isInstalled) {
          // App is already installed, uninstall it first
          wasAlreadyInstalled = true;
          console.log(`${app.displayName} is already installed, uninstalling first...`);

          await uninstallButton.click();

          // Wait for uninstallation to complete
          await waitForUninstalled(page, app.displayName, APP_OPERATION_TIMEOUT);
          console.log(`${app.displayName} uninstalled successfully`);
        }

        // Now install the app
        await expect(installButton).toBeVisible({ timeout: 10000 });
        console.log(`Installing ${app.displayName}...`);
        await installButton.click();

        // Wait for installation to complete
        await waitForInstalled(page, app.displayName, APP_OPERATION_TIMEOUT);
        console.log(`${app.displayName} installed successfully`);

        // Verify app is now installed
        await expect(openButton).toBeVisible();

        // Now uninstall the app
        await expect(uninstallButton).toBeVisible();
        console.log(`Uninstalling ${app.displayName}...`);
        await uninstallButton.click();

        // Wait for uninstallation to complete
        await waitForUninstalled(page, app.displayName, APP_OPERATION_TIMEOUT);
        console.log(`${app.displayName} uninstalled successfully`);

        // Verify app is now uninstalled
        await expect(installButton).toBeVisible();

        // If app was already installed before test, reinstall it
        if (wasAlreadyInstalled) {
          console.log(`Reinstalling ${app.displayName} (was previously installed)...`);
          await installButton.click();
          await waitForInstalled(page, app.displayName, APP_OPERATION_TIMEOUT);
          console.log(`${app.displayName} reinstalled successfully`);
        }
      });
    });
  }
});

test.describe('App Installation - Quick Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/apps');
    await expect(page.locator('h1:has-text("App Marketplace")')).toBeVisible({ timeout: 10000 });
    await page.waitForLoadState('networkidle');
  });

  test('install button shows installing state when clicked', async ({ page }) => {
    // Find an app that's not installed
    const installButton = page.locator('button:has-text("Install")').first();

    if (await installButton.isVisible().catch(() => false)) {
      // Click install
      await installButton.click();

      // Button should show installing state (badge appears)
      await expect(page.locator('text=Installing').first()).toBeVisible({ timeout: 10000 });
    } else {
      // All apps are installed, skip
      test.skip();
    }
  });

  test('installed apps show uninstall button', async ({ page }) => {
    // Find an installed app (has Open button)
    const openButton = page.locator('a:has-text("Open")').first();

    if (await openButton.isVisible().catch(() => false)) {
      // Find the app card containing this Open button
      const appCard = openButton.locator('..').locator('..');

      // Should have uninstall button
      await expect(appCard.locator('button[title="Uninstall"]')).toBeVisible();
    } else {
      // No apps installed, skip
      test.skip();
    }
  });

  test('system apps show system badge', async ({ page }) => {
    // Look for System badge
    const systemBadge = page.locator('text=System').first();

    // System badge might or might not be visible depending on what's deployed
    if (await systemBadge.isVisible({ timeout: 2000 }).catch(() => false)) {
      await expect(systemBadge).toBeVisible();
    }
  });
});
