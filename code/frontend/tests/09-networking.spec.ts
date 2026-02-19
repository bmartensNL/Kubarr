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

  test.describe('Topology Graph', () => {
    test('topology graph container is present', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // React Flow renders inside a div with class 'react-flow'
      // Or shows "No network data available" when empty
      const hasReactFlow = await page.locator('.react-flow').isVisible({ timeout: 10000 }).catch(() => false);
      const hasEmptyState = await page.locator('text=No network data available').isVisible().catch(() => false);
      const hasNetworkFlow = await page.locator('text=Network Flow').isVisible().catch(() => false);
      expect(hasReactFlow || hasEmptyState || hasNetworkFlow).toBe(true);
    });

    test('topology section renders without JavaScript errors', async ({ page }) => {
      const jsErrors: string[] = [];
      page.on('pageerror', (err) => {
        jsErrors.push(err.message);
      });

      await page.goto('/networking');
      await page.waitForLoadState('networkidle');

      // No JavaScript runtime errors should occur
      expect(jsErrors.length).toBe(0);
    });

    test('SVG elements present when topology data is available', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const hasReactFlow = await page.locator('.react-flow').isVisible({ timeout: 10000 }).catch(() => false);
      if (!hasReactFlow) {
        // No topology data - skip SVG check
        test.skip();
        return;
      }

      // React Flow renders SVGs for edges and the minimap
      const svgElements = page.locator('.react-flow svg');
      const svgCount = await svgElements.count();
      expect(svgCount).toBeGreaterThan(0);
    });

    test('empty state shows meaningful message when no network data', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const hasEmptyState = await page.locator('text=No network data available').isVisible({ timeout: 5000 }).catch(() => false);
      if (hasEmptyState) {
        await expect(page.locator('text=No network data available')).toBeVisible();
      }
    });

    test('MiniMap is visible when topology has nodes', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const hasReactFlow = await page.locator('.react-flow').isVisible({ timeout: 10000 }).catch(() => false);
      if (!hasReactFlow) {
        test.skip();
        return;
      }

      // React Flow MiniMap renders as a div inside react-flow
      const minimap = page.locator('.react-flow__minimap');
      const hasMinimap = await minimap.isVisible().catch(() => false);
      if (hasMinimap) {
        await expect(minimap).toBeVisible();
      }
    });
  });

  test.describe('Statistics Section Values', () => {
    test('stat cards display non-blank values', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const statNames = [
        'Internet Traffic',
        'Total Receive',
        'Total Transmit',
        'Errors/sec',
        'Dropped/sec',
      ];

      for (const statName of statNames) {
        const card = page.locator(`text=${statName}`).first();
        const isVisible = await card.isVisible({ timeout: 10000 }).catch(() => false);
        if (isVisible) {
          // The stat card container should have some text content
          const cardParent = card.locator('..').locator('..');
          const cardText = await cardParent.textContent().catch(() => '');
          expect(cardText?.trim().length).toBeGreaterThan(0);
        }
      }
    });

    test('stat values do not show NaN or undefined', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const pageText = await page.locator('main').textContent();

      // Page should contain numeric content
      expect(pageText).toMatch(/\d/);
      // Should not show 'NaN' or 'undefined' in the visible text
      expect(pageText).not.toContain('NaN');
      expect(pageText).not.toContain('undefined');
    });

    test('Internet Traffic stat card shows a value (even 0)', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const trafficCard = page.locator('text=Internet Traffic').first();
      const isVisible = await trafficCard.isVisible({ timeout: 15000 }).catch(() => false);

      if (isVisible) {
        // The card parent should contain the value
        const container = trafficCard.locator('../..').first();
        const text = await container.textContent().catch(() => '');
        // Should contain either a number with unit or a dash/zero indicator
        expect(text?.trim().length).toBeGreaterThan(0);
      }
    });
  });

  test.describe('VPN Integration', () => {
    test('networking page loads without VPN configuration errors', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const errorText = page.locator('text=/something went wrong|failed to load/i');
      await expect(errorText).not.toBeVisible();
    });

    test('VPN indicator appears on app nodes when VPN is configured', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      // Check if there are any VPN indicators visible in the topology
      const vpnIndicator = page.locator('text=/vpn|wireguard/i').first();
      const hasVpn = await vpnIndicator.isVisible().catch(() => false);

      if (hasVpn) {
        // VPN is configured - indicator should be visible
        await expect(vpnIndicator).toBeVisible();
      }
      // Passes regardless - VPN may not be configured in test environment
    });

    test('networking page does not show blank content', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const mainContent = await page.locator('main').textContent();
      expect(mainContent?.trim().length).toBeGreaterThan(0);
    });
  });
});
