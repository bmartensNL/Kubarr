import { test, expect, login, TEST_USERS } from './fixtures';

test.describe('Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.context().clearCookies();
    await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);
  });

  test.describe('Top Navigation Bar', () => {
    test('should display logo and brand name', async ({ page }) => {
      await expect(page.locator('a:has-text("Kubarr")')).toBeVisible();
    });

    test('should display Apps link in navigation', async ({ page }) => {
      await expect(page.locator('nav a:has-text("Apps")')).toBeVisible();
    });

    test('should display System dropdown in navigation', async ({ page }) => {
      // Use a more specific selector to avoid matching the theme "System" option
      await expect(page.locator('nav .hidden.md\\:flex button:has-text("System")')).toBeVisible();
    });

    test('should display Settings link in navigation', async ({ page }) => {
      await expect(page.locator('nav a:has-text("Settings")')).toBeVisible();
    });

    test('should display user menu with username', async ({ page }) => {
      await expect(page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first()).toBeVisible();
    });

    test('should display theme toggle button', async ({ page }) => {
      // Theme toggle button has title starting with "Theme:"
      await expect(page.locator('nav button[title^="Theme:"]').first()).toBeVisible();
    });
  });

  test.describe('Page Navigation', () => {
    test('should navigate to Apps page', async ({ page }) => {
      await page.click('nav a:has-text("Apps")');
      await expect(page).toHaveURL(/\/apps/);
      await expect(page.locator('text=Applications')).toBeVisible();
    });

    test('should navigate to Settings page', async ({ page }) => {
      await page.click('nav a:has-text("Settings")');
      await expect(page).toHaveURL(/\/settings/);
    });

    test('should navigate to Dashboard by clicking logo', async ({ page }) => {
      // First go to another page
      await page.goto('/apps');
      await expect(page).toHaveURL(/\/apps/);

      // Click logo to go back to dashboard
      await page.click('a:has-text("Kubarr")');
      await expect(page).toHaveURL(/\/$/);
    });
  });

  test.describe('System Dropdown', () => {
    test('should open System dropdown when clicked', async ({ page }) => {
      // Target the System button in the main nav (not the theme System option)
      await page.locator('nav .hidden.md\\:flex button:has-text("System")').click();

      // Dropdown should show menu items
      await expect(page.locator('a:has-text("Resources")')).toBeVisible();
      await expect(page.locator('a:has-text("Storage")')).toBeVisible();
      await expect(page.locator('a:has-text("Logs")')).toBeVisible();
    });

    test('should navigate to Resources page from System dropdown', async ({ page }) => {
      await page.locator('nav .hidden.md\\:flex button:has-text("System")').click();
      await page.click('a:has-text("Resources")');
      await expect(page).toHaveURL(/\/resources/);
    });

    test('should navigate to Storage page from System dropdown', async ({ page }) => {
      await page.locator('nav .hidden.md\\:flex button:has-text("System")').click();
      await page.click('a:has-text("Storage")');
      await expect(page).toHaveURL(/\/storage/);
    });

    test('should navigate to Logs page from System dropdown', async ({ page }) => {
      await page.locator('nav .hidden.md\\:flex button:has-text("System")').click();
      await page.click('a:has-text("Logs")');
      await expect(page).toHaveURL(/\/logs/);
    });
  });

  test.describe('User Menu', () => {
    test('should open user dropdown when clicked', async ({ page }) => {
      await page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first().click();

      // Dropdown should show options
      await expect(page.locator('text=Account Settings')).toBeVisible();
      await expect(page.locator('button:has-text("Logout")')).toBeVisible();
    });

    test('should navigate to Account page from user dropdown', async ({ page }) => {
      await page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first().click();
      await page.click('text=Account Settings');
      await expect(page).toHaveURL(/\/account/);
    });
  });

  test.describe('Theme Toggle', () => {
    test('should open theme dropdown when clicked', async ({ page }) => {
      // Click the theme toggle button (has title starting with "Theme:")
      await page.locator('nav button[title^="Theme:"]').first().click();

      // Should show theme options in dropdown
      await expect(page.locator('button:has-text("Light")')).toBeVisible();
      await expect(page.locator('button:has-text("Dark")')).toBeVisible();
      // "System" option in theme dropdown
      await expect(page.locator('button:has-text("System") >> nth=-1')).toBeVisible();
    });

    test('should switch to dark theme', async ({ page }) => {
      await page.locator('nav button[title^="Theme:"]').first().click();
      await page.locator('button:has-text("Dark")').click();

      // Body should have dark class
      await expect(page.locator('html')).toHaveClass(/dark/);
    });

    test('should switch to light theme', async ({ page }) => {
      // First switch to dark
      await page.locator('nav button[title^="Theme:"]').first().click();
      await page.locator('button:has-text("Dark")').click();
      await expect(page.locator('html')).toHaveClass(/dark/);

      // Then switch to light
      await page.locator('nav button[title^="Theme:"]').first().click();
      await page.locator('button:has-text("Light")').click();
      await expect(page.locator('html')).not.toHaveClass(/dark/);
    });
  });

  test.describe('404 Page', () => {
    test('should show 404 page for unknown routes', async ({ page }) => {
      await page.goto('/nonexistent-page');
      await expect(page.locator('text=404')).toBeVisible();
      await expect(page.locator('text=Page Not Found')).toBeVisible();
    });

    test('should have navigation back to home from 404 page', async ({ page }) => {
      await page.goto('/nonexistent-page');
      await page.click('a:has-text("Home")');
      await expect(page).toHaveURL(/\/$/);
    });
  });
});
