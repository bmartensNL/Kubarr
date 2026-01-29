import { test as setup, expect } from '@playwright/test';

/**
 * Bootstrap setup - drives the initial setup wizard (bootstrap, server config, admin user).
 * Only runs in CI where the app starts without a database.
 */
setup('bootstrap', async ({ page }) => {
  setup.skip(!process.env.CI, 'Bootstrap only runs in CI');

  // Navigate to the app - should redirect to /setup
  await page.goto('/');
  await expect(page).toHaveURL('/setup', { timeout: 30000 });

  // Step 1: Bootstrap - click "Start Setup" to begin installing components
  await expect(page.getByRole('button', { name: /start setup/i })).toBeVisible({ timeout: 10000 });
  await page.getByRole('button', { name: /start setup/i }).click();

  // Wait for all components to become healthy (PostgreSQL, VictoriaMetrics, etc.)
  // This can take several minutes as Helm charts are installed
  await expect(page.getByRole('button', { name: /continue/i })).toBeVisible({ timeout: 480000 });
  await page.getByRole('button', { name: /continue/i }).click();

  // Step 2: Server Configuration
  await expect(page.locator('input[name="name"], input[placeholder*="name" i]').first()).toBeVisible({ timeout: 10000 });
  await page.locator('input[name="name"], input[placeholder*="name" i]').first().fill('e2e-test');

  // Fill storage path
  const storageInput = page.locator('input[name="storage_path"], input[placeholder*="storage" i], input[placeholder*="path" i]').first();
  await storageInput.fill('/data');

  // Click next/continue
  const nextButton = page.getByRole('button', { name: /next|continue|save/i });
  await expect(nextButton).toBeEnabled({ timeout: 5000 });
  await nextButton.click();

  // Step 3: Admin User Creation
  await expect(page.locator('input[name="username"], input[placeholder*="username" i]').first()).toBeVisible({ timeout: 10000 });

  await page.locator('input[name="username"], input[placeholder*="username" i]').first().fill('admin');
  await page.locator('input[name="email"], input[type="email"]').first().fill('admin@test.local');
  await page.locator('input[name="password"], input[type="password"]').first().fill('adminadmin');

  // Fill confirm password if present
  const confirmPassword = page.locator('input[name="confirmPassword"], input[name="confirm_password"], input[placeholder*="confirm" i]');
  if (await confirmPassword.isVisible({ timeout: 1000 }).catch(() => false)) {
    await confirmPassword.fill('adminadmin');
  }

  // Click next
  const adminNextButton = page.getByRole('button', { name: /next|continue/i });
  await expect(adminNextButton).toBeEnabled({ timeout: 5000 });
  await adminNextButton.click();

  // Step 4: Summary - click "Complete Setup"
  await expect(page.getByRole('button', { name: /complete setup/i })).toBeVisible({ timeout: 10000 });
  await page.getByRole('button', { name: /complete setup/i }).click();

  // Wait for redirect to login or dashboard
  await expect(page).not.toHaveURL('/setup', { timeout: 30000 });
});
