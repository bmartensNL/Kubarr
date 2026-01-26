import { test, expect, login, logout, TEST_USERS } from './fixtures';

test.describe('Authentication', () => {
  test.describe('Login Flow', () => {
    test('should redirect unauthenticated user to OAuth login', async ({ page }) => {
      // Clear cookies to ensure clean state
      await page.context().clearCookies();

      // Navigate to protected route
      await page.goto('/');

      // Should be redirected to OAuth login
      await expect(page).toHaveURL(/\/auth\/login/);

      // Verify login form elements are present
      await expect(page.locator('input[name="username"]')).toBeVisible();
      await expect(page.locator('input[name="password"]')).toBeVisible();
      await expect(page.locator('button[type="submit"]')).toBeVisible();
    });

    test('should login successfully with valid credentials', async ({ page }) => {
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Verify we're on the dashboard
      await expect(page).toHaveURL(/\/$/);

      // Verify user menu shows the username
      await expect(page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first()).toBeVisible();
    });

    test('should show error for invalid credentials', async ({ page }) => {
      await page.context().clearCookies();
      await page.goto('/');

      // Wait for login page
      await page.waitForURL(/\/auth\/login/);

      // Enter invalid credentials
      await page.fill('input[name="username"]', 'invalid_user');
      await page.fill('input[name="password"]', 'invalid_password');

      // Submit the form
      await page.evaluate(() => {
        const form = document.querySelector('form');
        if (form) form.submit();
      });

      // Wait a moment for the response
      await page.waitForTimeout(2000);

      // Should still be on login page (or show error)
      await expect(page).toHaveURL(/\/auth\/login/);
    });
  });

  test.describe('Logout Flow', () => {
    test('should logout and redirect to login page', async ({ page }) => {
      // First login
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Verify logged in
      await expect(page).toHaveURL(/\/$/);

      // Perform logout
      await logout(page);

      // Should be redirected to login page (either frontend /login or OAuth /auth/login)
      const url = page.url();
      expect(url.includes('/login') || url.includes('/auth/login')).toBeTruthy();
    });

    test('should not be able to access protected routes after logout', async ({ page }) => {
      // Login first
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Logout
      await logout(page);

      // Try to access protected route
      await page.goto('/apps');

      // Should be redirected to OAuth login
      await expect(page).toHaveURL(/\/auth\/login/);
    });

    test('should be able to access OAuth login page after logout', async ({ page }) => {
      // Login
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Logout
      await logout(page);

      // Clear cookies completely and navigate to protected route
      await page.context().clearCookies();
      await page.goto('/apps', { waitUntil: 'networkidle' });

      // Should be redirected to OAuth login
      await expect(page).toHaveURL(/\/auth\/login/);

      // Should see login form
      await expect(page.locator('input[name="username"]')).toBeVisible();
      await expect(page.locator('input[name="password"]')).toBeVisible();
    });
  });

  test.describe('Session Persistence', () => {
    test('should maintain session across page refreshes', async ({ page }) => {
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Refresh the page
      await page.reload();

      // Should still be logged in
      await expect(page).toHaveURL(/\/$/);
      await expect(page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first()).toBeVisible();
    });

    test('should maintain session when navigating between pages', async ({ page }) => {
      await page.context().clearCookies();
      await login(page, TEST_USERS.admin.username, TEST_USERS.admin.password);

      // Navigate to different pages
      await page.goto('/apps');
      await expect(page).toHaveURL(/\/apps/);

      await page.goto('/settings');
      await expect(page).toHaveURL(/\/settings/);

      await page.goto('/');
      await expect(page).toHaveURL(/\/$/);

      // Should still be logged in
      await expect(page.locator(`button:has-text("${TEST_USERS.admin.username}")`).first()).toBeVisible();
    });
  });
});
