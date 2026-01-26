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
 * Helper to perform login via the OAuth login form
 */
export async function login(page: Page, username: string, password: string): Promise<void> {
  console.log('Starting login flow...');

  // Navigate to the app - this will redirect to OAuth login if not authenticated
  await page.goto('/');

  // Log current URL
  console.log('Initial URL:', page.url());

  // Wait for either dashboard (already logged in) or login page
  try {
    await page.waitForURL(/\/(auth\/login|login|$)/, { timeout: 30000 });
    console.log('After waitForURL:', page.url());
  } catch (e) {
    console.log('waitForURL timeout, current URL:', page.url());
    // Take a screenshot for debugging
    await page.screenshot({ path: 'test-results/login-timeout.png' });
    throw e;
  }

  // Check current URL to determine which login flow to use
  const url = page.url();
  console.log('Current URL:', url);

  if (url.includes('/auth/login')) {
    console.log('On OAuth login page, filling credentials...');

    // Wait for form to be visible
    await page.waitForSelector('input[name="username"]', { timeout: 10000 });

    // Fill in credentials
    await page.fill('input[name="username"]', username);
    await page.fill('input[name="password"]', password);

    console.log('Submitting form...');

    // Click submit button instead of form.submit() for better compatibility
    const submitButton = page.locator('button[type="submit"]');
    if (await submitButton.isVisible()) {
      await submitButton.click();
    } else {
      // Fallback to form submission
      await page.evaluate(() => {
        const form = document.querySelector('form');
        if (form) form.submit();
      });
    }

    // Wait for redirect - could be to dashboard or error back to login
    try {
      await page.waitForURL(/\/$/, { timeout: 30000 });
      console.log('Redirected to dashboard:', page.url());
    } catch (e) {
      console.log('Redirect timeout, current URL:', page.url());
      const pageContent = await page.content();
      if (pageContent.includes('error') || pageContent.includes('Error')) {
        console.log('Error detected on page');
      }
      await page.screenshot({ path: 'test-results/login-redirect-timeout.png' });
      throw e;
    }
  } else if (url.includes('/login') && !url.includes('/auth/login')) {
    console.log('On frontend login page, triggering OAuth flow...');

    // Clear cookies and try OAuth flow directly
    await page.context().clearCookies();
    await page.goto('/oauth2/start', { waitUntil: 'networkidle' });
    await page.waitForTimeout(2000);

    // Wait for OAuth login page
    await page.waitForURL(/\/auth\/login/, { timeout: 30000 });
    console.log('Redirected to OAuth login:', page.url());

    // Fill the OAuth login form
    await page.fill('input[name="username"]', username);
    await page.fill('input[name="password"]', password);

    const submitButton = page.locator('button[type="submit"]');
    if (await submitButton.isVisible()) {
      await submitButton.click();
    } else {
      await page.evaluate(() => {
        const form = document.querySelector('form');
        if (form) form.submit();
      });
    }

    await page.waitForURL(/\/$/, { timeout: 30000 });
  } else if (url.match(/\/$/)) {
    console.log('Already on dashboard, assuming logged in');
  }

  // Verify we're logged in by checking for dashboard content
  // The nav bar should be visible when logged in
  console.log('Verifying login succeeded...');
  const currentUrl = page.url();
  console.log('Final URL:', currentUrl);

  // Wait for the navigation to be visible (indicates app has loaded)
  const navBar = page.locator('nav');
  await expect(navBar).toBeVisible({ timeout: 15000 });

  // Look for user menu or dashboard content
  const loggedInIndicator = page.locator([
    `button:has-text("${username}")`,          // User menu button with username
    'text=Kubarr',                             // Kubarr branding visible when logged in
    'a[href="/apps"]',                         // Apps link in nav
  ].join(', ')).first();

  try {
    await expect(loggedInIndicator).toBeVisible({ timeout: 15000 });
    console.log('Login verification passed');
  } catch (e) {
    console.log('Login verification failed');
    const html = await page.content();
    console.log('Page contains "admin":', html.includes('admin'));
    console.log('Page contains "Kubarr":', html.includes('Kubarr'));
    await page.screenshot({ path: 'test-results/login-verification-failed.png' });
    throw e;
  }
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
