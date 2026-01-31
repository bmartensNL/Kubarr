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
  // The actual inputs use id="serverName" and id="storagePath" with no name attributes
  const serverNameInput = page.locator('input#serverName, input[placeholder*="Server" i]').first();
  await expect(serverNameInput).toBeVisible({ timeout: 30000 });
  await serverNameInput.fill('e2e-test');

  // Fill storage path - use /tmp which exists and is writable in all containers
  const storageInput = page.locator('input#storagePath, input[placeholder*="kubarr" i]').first();
  await storageInput.fill('/tmp');

  // Click the Validate button to explicitly validate the path before proceeding.
  // This avoids a race condition where onBlur fires simultaneously with the Next click,
  // potentially disabling the button mid-click.
  const validateButton = page.getByRole('button', { name: /validate/i });
  await validateButton.click();

  // Wait for validation to complete (green check or error message appears)
  await expect(page.locator('.text-green-400, .text-red-400')).toBeVisible({ timeout: 15000 });

  // Click next/continue
  const nextButton = page.getByRole('button', { name: /^next$/i });
  await expect(nextButton).toBeEnabled({ timeout: 10000 });
  await nextButton.click();

  // Wait for the server configuration API call to complete
  await page.waitForTimeout(2000);

  // Step 3: Admin User Creation
  // Inputs use id-based selectors: adminUsername, adminEmail, adminPassword, confirmPassword
  const usernameInput = page.locator('input#adminUsername, input[placeholder*="username" i]').first();
  await expect(usernameInput).toBeVisible({ timeout: 30000 });

  await usernameInput.fill('admin');
  await page.locator('input#adminEmail, input[type="email"]').first().fill('admin@test.local');
  await page.locator('input#adminPassword, input[type="password"]').first().fill('adminadmin');

  // Fill confirm password if present
  const confirmPassword = page.locator('input#confirmPassword, input[placeholder*="confirm" i]');
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
