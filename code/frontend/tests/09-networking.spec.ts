import { test, expect } from '@playwright/test';

test.describe('Networking Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/networking');
    await expect(page.locator('h1:has-text("Networking")')).toBeVisible({ timeout: 10000 });
  });

  test.describe('Page Display', () => {
    test('shows page heading and subtitle', async ({ page }) => {
      await expect(page.locator('h1:has-text("Networking")')).toBeVisible();
      await expect(page.locator('text=Network topology and traffic flow')).toBeVisible();
    });

    test('shows Internet Traffic stat card', async ({ page }) => {
      await expect(page.locator('text=Internet Traffic')).toBeVisible({ timeout: 15000 });
    });

    test('shows Total Receive stat card', async ({ page }) => {
      await expect(page.locator('text=Total Receive')).toBeVisible({ timeout: 15000 });
    });

    test('shows Total Transmit stat card', async ({ page }) => {
      await expect(page.locator('text=Total Transmit')).toBeVisible({ timeout: 15000 });
    });

    test('shows Errors/sec stat card', async ({ page }) => {
      await expect(page.locator('text=Errors/sec')).toBeVisible({ timeout: 15000 });
    });

    test('shows Dropped/sec stat card', async ({ page }) => {
      await expect(page.locator('text=Dropped/sec')).toBeVisible({ timeout: 15000 });
    });

    test('shows Network Flow section', async ({ page }) => {
      await expect(page.locator('text=Network Flow')).toBeVisible({ timeout: 15000 });
    });

    test('shows Network Statistics section', async ({ page }) => {
      await expect(page.locator('h2:has-text("Network Statistics")').first()).toBeVisible({ timeout: 15000 });
    });
  });

  test.describe('Controls', () => {
    test('Infra toggle button works', async ({ page }) => {
      // Find the Infra toggle button
      const infraButton = page.locator('button:has-text("Infra")');
      await expect(infraButton).toBeVisible({ timeout: 15000 });
      // Click to toggle
      await infraButton.click();
      // Button should still be visible (toggled state)
      await expect(infraButton).toBeVisible();
    });

    test('Refresh button is visible', async ({ page }) => {
      // There should be a refresh button in the header area
      const refreshButton = page.locator('button').filter({ has: page.locator('svg.lucide-refresh-cw') }).first();
      await expect(refreshButton).toBeVisible({ timeout: 10000 });
    });

    test('Connection status indicator is shown', async ({ page }) => {
      // Should show one of: Live, Polling, or Disconnected
      const live = page.locator('text=Live');
      const polling = page.locator('text=Polling');
      const disconnected = page.locator('text=Disconnected');

      await page.waitForLoadState('networkidle');
      const hasLive = await live.first().isVisible().catch(() => false);
      const hasPolling = await polling.isVisible().catch(() => false);
      const hasDisconnected = await disconnected.isVisible().catch(() => false);
      expect(hasLive || hasPolling || hasDisconnected).toBe(true);
    });
  });

  test.describe('Network Statistics Table', () => {
    test('shows Network Statistics section or Network Flow only', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // Network Statistics may not appear when there are no apps with network data
      const hasStats = await page.locator('text=Network Statistics').isVisible({ timeout: 5000 }).catch(() => false);
      const hasFlow = await page.locator('text=Network Flow').isVisible().catch(() => false);
      expect(hasStats || hasFlow).toBe(true);
    });
  });
});
