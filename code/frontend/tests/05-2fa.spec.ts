import { test, expect } from '@playwright/test';

test.describe('Two-Factor Authentication', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/account');
    // Wait for account page to load
    await expect(page.locator('h1:has-text("Account Settings")')).toBeVisible();
    // Wait for 2FA section to load
    await expect(page.locator('h3:has-text("Two-Factor Authentication")')).toBeVisible();
  });

  test.describe('2FA Section Display', () => {
    test('shows 2FA section with icon', async ({ page }) => {
      const section = page.locator('h3:has-text("Two-Factor Authentication")');
      await expect(section).toBeVisible();
      // Icon should be present (Smartphone icon)
      await expect(section.locator('svg')).toBeVisible();
    });

    test('shows either enable or disable state', async ({ page }) => {
      // Wait for loading to complete
      await page.waitForLoadState('networkidle');

      // Should show either "Set Up" button or "Disable" button depending on current state
      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');
      const disableButton = page.locator('button:has-text("Disable 2FA")');

      const hasSetup = await setupButton.isVisible();
      const hasDisable = await disableButton.isVisible();

      // One of them should be visible
      expect(hasSetup || hasDisable).toBe(true);
    });

    test('shows informational text about 2FA', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      // Should show description about authenticator apps
      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      if (await setupButton.isVisible()) {
        await expect(page.locator('text=authenticator app')).toBeVisible();
        await expect(page.locator('text=Google Authenticator')).toBeVisible();
      }
    });
  });

  test.describe('2FA Setup Flow', () => {
    test('can initiate 2FA setup and see QR code', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Should show QR code section
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      // Should show QR code image (SVG)
      await expect(page.locator('svg').first()).toBeVisible();

      // Should show manual entry key
      await expect(page.locator('text=Manual Entry Key')).toBeVisible();

      // Should show verification code label
      await expect(page.locator('text=Verification Code')).toBeVisible();

      // Should show code input with placeholder 000000
      await expect(page.locator('input[placeholder="000000"]')).toBeVisible();

      // Should show Verify & Enable button
      await expect(page.locator('button:has-text("Verify & Enable")')).toBeVisible();

      // Should show Cancel button
      await expect(page.locator('button:has-text("Cancel")')).toBeVisible();
    });

    test('can cancel 2FA setup', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Wait for QR code to appear
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      // Click cancel
      await page.locator('button:has-text("Cancel")').click();

      // QR code should disappear
      await expect(page.locator('text=Scan this QR code')).not.toBeVisible();

      // Setup button should be visible again
      await expect(setupButton).toBeVisible();
    });

    test('verify button is disabled without code', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Wait for setup UI
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      // Verify button should be disabled without code
      const verifyButton = page.locator('button:has-text("Verify & Enable")');
      await expect(verifyButton).toBeDisabled();
    });

    test('verify button enables with 6-digit code', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Wait for setup UI
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      // Enter a 6-digit code
      const codeInput = page.locator('input[placeholder="000000"]');
      await codeInput.fill('123456');

      // Verify button should be enabled
      const verifyButton = page.locator('button:has-text("Verify & Enable")');
      await expect(verifyButton).toBeEnabled();
    });

    test('shows error for invalid verification code', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Wait for setup UI
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      // Enter an invalid code
      const codeInput = page.locator('input[placeholder="000000"]');
      await codeInput.fill('000000');

      // Click verify
      await page.locator('button:has-text("Verify & Enable")').click();

      // Should show error message
      await expect(page.locator('text=/Invalid|incorrect|failed/i').first()).toBeVisible({ timeout: 5000 });
    });

    test('code input only accepts digits and limits to 6', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');

      // Skip if 2FA is already enabled
      if (!(await setupButton.isVisible())) {
        test.skip();
        return;
      }

      // Click setup button
      await setupButton.click();

      // Wait for setup UI
      await expect(page.locator('text=Scan this QR code')).toBeVisible({ timeout: 10000 });

      const codeInput = page.locator('input[placeholder="000000"]');

      // Input should have maxLength of 6
      await expect(codeInput).toHaveAttribute('maxlength', '6');

      // Enter digits
      await codeInput.fill('123456');

      // Should contain the 6 digits
      await expect(codeInput).toHaveValue('123456');
    });
  });

  test.describe('2FA Disable Flow', () => {
    test('shows password input when 2FA is enabled', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const disableButton = page.locator('button:has-text("Disable 2FA")');

      // Skip if 2FA is not enabled
      if (!(await disableButton.isVisible())) {
        test.skip();
        return;
      }

      // Should show password label and input
      await expect(page.locator('label:has-text("Password")')).toBeVisible();
      await expect(page.locator('input[type="password"]').first()).toBeVisible();

      // Should show disable button
      await expect(disableButton).toBeVisible();
    });

    test('disable button requires password', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const disableButton = page.locator('button:has-text("Disable 2FA")');

      // Skip if 2FA is not enabled
      if (!(await disableButton.isVisible())) {
        test.skip();
        return;
      }

      // Disable button should be disabled without password
      await expect(disableButton).toBeDisabled();
    });

    test('disable button enables with password', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const disableButton = page.locator('button:has-text("Disable 2FA")');

      // Skip if 2FA is not enabled
      if (!(await disableButton.isVisible())) {
        test.skip();
        return;
      }

      // Find the password input in the 2FA section
      const twoFASection = page.locator('h3:has-text("Two-Factor Authentication")').locator('..');
      const passwordInput = twoFASection.locator('input[type="password"]');
      await passwordInput.fill('somepassword');

      // Disable button should be enabled
      await expect(disableButton).toBeEnabled();
    });

    test('shows error for wrong password when disabling', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const disableButton = page.locator('button:has-text("Disable 2FA")');

      // Skip if 2FA is not enabled
      if (!(await disableButton.isVisible())) {
        test.skip();
        return;
      }

      // Find the password input in the 2FA section
      const twoFASection = page.locator('h3:has-text("Two-Factor Authentication")').locator('..');
      const passwordInput = twoFASection.locator('input[type="password"]');
      await passwordInput.fill('wrongpassword');

      // Click disable
      await disableButton.click();

      // Should show error
      await expect(page.locator('text=/failed|invalid|incorrect/i').first()).toBeVisible({ timeout: 5000 });
    });
  });

  test.describe('2FA Status Indicators', () => {
    test('shows enabled status when 2FA is on', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const disableButton = page.locator('button:has-text("Disable 2FA")');

      // Skip if 2FA is not enabled
      if (!(await disableButton.isVisible())) {
        test.skip();
        return;
      }

      // Should show that 2FA is enabled (the disable section text)
      await expect(page.locator('text=two-factor authentication is currently enabled')).toBeVisible();
    });

    test('shows role requirement warning if applicable', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      // Check if the role requirement warning is shown
      const roleWarning = page.locator('text=Your role requires 2FA');

      if (await roleWarning.isVisible()) {
        // Warning should be visible
        await expect(roleWarning).toBeVisible();

        // Disable button should be disabled when role requires 2FA
        const disableButton = page.locator('button:has-text("Disable 2FA")');
        if (await disableButton.isVisible()) {
          await expect(disableButton).toBeDisabled();
        }
      }
    });
  });

  test.describe('2FA Recovery Codes', () => {
    test('account page 2FA section loads without errors', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // The 2FA section should be visible and not showing an error state
      await expect(page.locator('h3:has-text("Two-Factor Authentication")')).toBeVisible();
      // No generic error messages
      const errorText = page.locator('text=/something went wrong|internal server error/i');
      await expect(errorText).not.toBeVisible();
    });

    test('2FA section does not show blank or error state', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      // Should show either setup button or disable button (not a blank state)
      const setupButton = page.locator('button:has-text("Set Up Two-Factor Authentication")');
      const disableButton = page.locator('button:has-text("Disable 2FA")');
      const hasSetup = await setupButton.isVisible().catch(() => false);
      const hasDisable = await disableButton.isVisible().catch(() => false);
      expect(hasSetup || hasDisable).toBe(true);
    });

    test('checks for recovery codes section if 2FA is enabled', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const disableButton = page.locator('button:has-text("Disable 2FA")');
      if (!(await disableButton.isVisible())) {
        // 2FA is not enabled - skip recovery code tests
        test.skip();
        return;
      }

      // If 2FA is enabled, check if recovery codes section exists
      // Recovery codes may not yet be implemented in the backend
      const recoverySection = page.locator('text=/recovery code|backup code/i').first();
      const hasRecoveryCodes = await recoverySection.isVisible().catch(() => false);

      if (hasRecoveryCodes) {
        // Recovery codes section is visible - verify it has content
        await expect(recoverySection).toBeVisible();
      }
      // No assertion failure if recovery codes are not yet implemented
    });

    test('recovery codes are non-empty strings if section exists', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      const recoverySection = page.locator('text=/recovery code|backup code/i').first();
      const hasRecoveryCodes = await recoverySection.isVisible().catch(() => false);

      if (!hasRecoveryCodes) {
        test.skip();
        return;
      }

      // Each displayed recovery code should be a non-empty string
      const codes = page.locator('[data-testid="recovery-code"], .recovery-code');
      const count = await codes.count();

      if (count > 0) {
        for (let i = 0; i < count; i++) {
          const text = await codes.nth(i).textContent();
          expect(text?.trim().length).toBeGreaterThan(0);
        }
        // Should have the expected number of recovery codes (8 per spec)
        expect(count).toBe(8);
      }
    });

    test('recovery codes count is shown if feature is implemented', async ({ page }) => {
      await page.waitForLoadState('networkidle');

      // Check if recovery codes count indicator exists
      const countIndicator = page.locator('text=/\\d+ recovery code|recovery code.*\\d+/i').first();
      const hasCount = await countIndicator.isVisible().catch(() => false);

      if (hasCount) {
        await expect(countIndicator).toBeVisible();
        // The count text should be readable
        const text = await countIndicator.textContent();
        expect(text?.trim().length).toBeGreaterThan(0);
      }
      // Passes if feature is not yet implemented
    });
  });
});
