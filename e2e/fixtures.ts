import { test as base, expect, Page } from '@playwright/test';

/**
 * Test credentials - these should match the test environment
 */
export const TEST_USERS = {
  admin: {
    username: 'admin',
    password: 'admin',
  },
};

/**
 * Helper to perform login via the Hydra OAuth login form
 */
export async function login(page: Page, username: string, password: string): Promise<void> {
  // Navigate to the app - this will redirect to OAuth login if not authenticated
  await page.goto('/');

  // Wait for either dashboard (already logged in) or some login page
  await page.waitForURL(/\/(auth\/login|login|$)/, { timeout: 30000 });

  // Check current URL to determine which login flow to use
  const url = page.url();

  if (url.includes('/auth/login')) {
    // OAuth login page (Hydra)
    await page.fill('input[name="username"]', username);
    await page.fill('input[name="password"]', password);

    // Submit the form
    await page.evaluate(() => {
      const form = document.querySelector('form');
      if (form) form.submit();
    });

    // Wait for redirect to dashboard
    await page.waitForURL(/\/$/, { timeout: 30000 });
  } else if (url.includes('/login') && !url.includes('/auth/login')) {
    // Frontend login page - need to trigger OAuth flow
    // Clear cookies and reload to force fresh state
    await page.context().clearCookies();

    // Navigate directly to OAuth endpoint to trigger OAuth flow
    await page.goto('/oauth2/start', { waitUntil: 'networkidle' });
    await page.waitForTimeout(2000);

    // Wait for OAuth login page
    await page.waitForURL(/\/auth\/login/, { timeout: 30000 });

    // Now fill the OAuth login form
    await page.fill('input[name="username"]', username);
    await page.fill('input[name="password"]', password);

    await page.evaluate(() => {
      const form = document.querySelector('form');
      if (form) form.submit();
    });

    await page.waitForURL(/\/$/, { timeout: 30000 });
  }

  // Verify we're logged in by checking for user menu
  await expect(page.locator(`button:has-text("${username}")`).first()).toBeVisible({ timeout: 10000 });
}

/**
 * Helper to perform logout
 */
export async function logout(page: Page): Promise<void> {
  // Clear all cookies to ensure clean logout
  await page.context().clearCookies();

  // Navigate to root - should redirect to OAuth login since cookies are cleared
  await page.goto('/', { waitUntil: 'networkidle' });

  // Wait a moment for any redirects
  await page.waitForTimeout(1000);

  // Verify we're at the OAuth login page
  const url = page.url();
  if (!url.includes('/auth/login')) {
    // If not redirected, try one more navigation
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForTimeout(1000);
  }
}

/**
 * Extended test fixture with login helper
 */
export const test = base.extend<{
  authenticatedPage: Page;
}>({
  authenticatedPage: async ({ page }, use) => {
    await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);
    await use(page);
  },
});

export { expect };
