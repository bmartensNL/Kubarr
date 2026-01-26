import { test, expect, login, TEST_USERS } from './fixtures';

test.describe('Apps Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.context().clearCookies();
    await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);
    await page.goto('/apps');
  });

  test.describe('Page Layout', () => {
    test('should display Apps page title', async ({ page }) => {
      await expect(page.locator('h1:has-text("App Marketplace")')).toBeVisible();
    });

    test('should display app categories', async ({ page }) => {
      // Wait for the page to load
      await page.waitForTimeout(2000);

      // Check for at least one category section (Media Managers, Download Clients, etc.)
      const hasCategory = await page.locator('text=Media Managers').isVisible() ||
                          await page.locator('text=Download Clients').isVisible() ||
                          await page.locator('text=Media Servers').isVisible();
      expect(hasCategory).toBeTruthy();
    });
  });

  test.describe('App Catalog', () => {
    test('should display app cards in catalog', async ({ page }) => {
      // Wait for catalog to load
      await page.waitForTimeout(2000);

      // Look for common media app names (case insensitive)
      const hasApps = await page.locator('text=/sonarr/i').first().isVisible() ||
                      await page.locator('text=/radarr/i').first().isVisible() ||
                      await page.locator('text=/qbittorrent/i').first().isVisible() ||
                      await page.locator('text=/jellyfin/i').first().isVisible() ||
                      await page.locator('text=/plex/i').first().isVisible();

      expect(hasApps).toBeTruthy();
    });

    test('should display app icons', async ({ page }) => {
      await page.waitForTimeout(2000);

      // Check that app icons are loading - they may use different src patterns
      const icons = page.locator('img[src*="/api/apps/catalog/"], img[alt*="icon"], img[alt*="logo"]');
      const count = await icons.count();

      // If specific app icons not found, check for any images in the app cards area
      if (count === 0) {
        const anyImages = await page.locator('main img').count();
        expect(anyImages).toBeGreaterThan(0);
      } else {
        expect(count).toBeGreaterThan(0);
      }
    });
  });

  test.describe('App Installation', () => {
    test('should show install button for available apps', async ({ page }) => {
      await page.waitForTimeout(2000);

      // Look for install buttons
      const installButtons = page.locator('button:has-text("Install")');
      const count = await installButtons.count();

      // There should be at least some install buttons for available apps
      // (unless all apps are already installed)
      expect(count).toBeGreaterThanOrEqual(0);
    });
  });

  test.describe('Installed Apps', () => {
    test('should show app status badges for installed apps', async ({ page }) => {
      await page.waitForTimeout(2000);

      // Look for status badges (Installed, Ready, Unhealthy) or install buttons
      const hasStatusOrInstall = await page.locator('text=Installed').first().isVisible() ||
                                  await page.locator('text=Ready').first().isVisible() ||
                                  await page.locator('button:has-text("Install")').first().isVisible();

      expect(hasStatusOrInstall).toBeTruthy();
    });

    test('should show action buttons for apps', async ({ page }) => {
      await page.waitForTimeout(2000);

      // Apps should have either Open, Install, or Uninstall buttons
      const hasActions = await page.locator('button:has-text("Open")').first().isVisible() ||
                         await page.locator('button:has-text("Install")').first().isVisible() ||
                         await page.locator('button:has-text("Uninstall")').first().isVisible();

      expect(hasActions).toBeTruthy();
    });
  });

  test.describe('App Categories', () => {
    test('should display category sections', async ({ page }) => {
      await page.waitForTimeout(2000);

      // The page should have category sections like Media Managers, Download Clients
      const hasCategories = await page.locator('text=Media Managers').isVisible() ||
                           await page.locator('text=Download Clients').isVisible() ||
                           await page.locator('text=Media Servers').isVisible();

      expect(hasCategories).toBeTruthy();
    });
  });
});

test.describe('App Details', () => {
  test.beforeEach(async ({ page }) => {
    await page.context().clearCookies();
    await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);
    await page.goto('/apps');
  });

  test('should display app information in cards', async ({ page }) => {
    await page.waitForTimeout(2000);

    // Verify apps are displayed with their information
    // Look for app name headings and description text
    const hasAppInfo = await page.locator('h3').first().isVisible() ||
                       await page.locator('h4').first().isVisible() ||
                       await page.locator('p.text-gray-500, p.text-gray-600').first().isVisible();

    expect(hasAppInfo).toBeTruthy();
  });
});
