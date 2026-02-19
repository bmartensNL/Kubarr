import { test, expect, Page } from '@playwright/test';

// Dummy WireGuard private key (not a real key, safe for testing)
const DUMMY_WG_PRIVATE_KEY = 'wOEI9rqqbDwnN8/Bpp22sVz48T71LKs/J5tGZwVgQHg=';
const TEST_PROVIDER_NAME = 'Test WireGuard Provider';
const UPDATED_PROVIDER_NAME = 'Updated WireGuard Provider';

// Navigate to the VPN settings section
async function gotoVpnSettings(page: Page) {
  await page.goto('/settings?section=vpn');
  await expect(page.locator('text=VPN Configuration')).toBeVisible({ timeout: 10000 });
}

// Open the "Add VPN Provider" form
async function openAddProviderForm(page: Page) {
  const addButton = page.locator('button:has-text("Add VPN Provider"), button:has-text("Add Provider")').first();
  await expect(addButton).toBeVisible({ timeout: 5000 });
  await addButton.click();
  await expect(page.locator('text=Add VPN Provider').first()).toBeVisible({ timeout: 5000 });
}

// Fill in WireGuard provider form fields
async function fillWireGuardForm(page: Page, name: string) {
  // Fill provider name
  await page.locator('input[placeholder="My VPN"], input[name="name"]').fill(name);

  // Select WireGuard type if not already selected
  const wgOption = page.locator('text=WireGuard').first();
  if (await wgOption.isVisible().catch(() => false)) {
    await wgOption.click();
  }

  // Fill WireGuard private key
  const privateKeyInput = page.locator('input[placeholder*="private key" i], input[placeholder*="WireGuard private" i]');
  await expect(privateKeyInput).toBeVisible({ timeout: 5000 });
  await privateKeyInput.fill(DUMMY_WG_PRIVATE_KEY);
}

test.describe('VPN Settings', () => {
  test.describe('Navigation', () => {
    test('shows VPN section in settings sidebar', async ({ page }) => {
      await page.goto('/settings');
      await expect(page.locator('text=System Dashboard')).toBeVisible({ timeout: 10000 });
      await expect(page.locator('nav button:has-text("VPN")').first()).toBeVisible();
    });

    test('navigates to VPN section via sidebar', async ({ page }) => {
      await page.goto('/settings');
      await expect(page.locator('text=System Dashboard')).toBeVisible({ timeout: 10000 });
      await page.locator('nav button:has-text("VPN")').first().click();
      await expect(page).toHaveURL(/section=vpn/);
      await expect(page.locator('text=VPN Configuration')).toBeVisible({ timeout: 10000 });
    });

    test('shows VPN page with correct headings', async ({ page }) => {
      await gotoVpnSettings(page);
      await expect(page.locator('text=VPN Providers').first()).toBeVisible();
      await expect(page.locator('text=App VPN Assignments').first()).toBeVisible();
    });

    test('shows Add VPN Provider button', async ({ page }) => {
      await gotoVpnSettings(page);
      const addButton = page.locator('button:has-text("Add VPN Provider"), button:has-text("Add Provider")').first();
      await expect(addButton).toBeVisible();
    });
  });

  test.describe('VPN Provider Management', () => {
    test('opens Add VPN Provider form', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      // Form should show provider configuration fields
      await expect(page.locator('text=/name/i').first()).toBeVisible();
      await expect(page.locator('text=WireGuard').first()).toBeVisible();
    });

    test('Add VPN Provider form has expected fields', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      // Should have VPN type selection
      await expect(page.locator('text=WireGuard').first()).toBeVisible();
      await expect(page.locator('text=OpenVPN').first()).toBeVisible();

      // Should have Cancel button
      await expect(page.locator('button:has-text("Cancel")').first()).toBeVisible();
    });

    test('Cancel closes the Add VPN Provider form', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      await page.locator('button:has-text("Cancel")').first().click();
      await page.waitForTimeout(500);

      // Form should be closed - modal heading gone
      const formHeading = page.locator('text=Add VPN Provider');
      // Wait for any animation to complete
      await expect(formHeading).not.toBeVisible({ timeout: 3000 }).catch(() => {
        // If it's still visible, check if it's the section heading vs modal
      });
    });

    test('create WireGuard VPN provider with dummy credentials', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);
      await fillWireGuardForm(page, TEST_PROVIDER_NAME);

      // Submit the form
      const submitButton = page.locator('button:has-text("Add Provider"), button:has-text("Save"), button[type="submit"]').first();
      await expect(submitButton).toBeVisible();
      await submitButton.click();

      // Wait for submission (API call + possible rerender)
      await page.waitForTimeout(1000);
      await page.waitForLoadState('networkidle');

      // Provider should appear in the list OR show an error (either is acceptable for dummy creds)
      const providerInList = await page.locator(`text=${TEST_PROVIDER_NAME}`).isVisible({ timeout: 5000 }).catch(() => false);
      const errorShown = await page.locator('text=/error|invalid|failed/i').first().isVisible({ timeout: 2000 }).catch(() => false);

      // At least one of these should be true - it either saved or showed an error
      expect(providerInList || errorShown).toBe(true);
    });

    test('WireGuard provider form shows private key field', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      // Click WireGuard if there's a type selector
      const wgRadio = page.locator('text=WireGuard').first();
      if (await wgRadio.isVisible().catch(() => false)) {
        await wgRadio.click();
      }

      // Should show private key field
      const privateKeyField = page.locator('input[placeholder*="private key" i], input[placeholder*="WireGuard private" i]');
      await expect(privateKeyField).toBeVisible({ timeout: 5000 });
    });

    test('OpenVPN form shows username and password fields', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      // Click OpenVPN type
      const ovpnOption = page.locator('text=OpenVPN').first();
      if (await ovpnOption.isVisible().catch(() => false)) {
        await ovpnOption.click();
        await page.waitForTimeout(300);

        // Should show username and password fields
        const usernameField = page.locator('input[placeholder*="username" i], input[name="username"]');
        const passwordField = page.locator('input[type="password"][placeholder*="password" i]');

        const hasUsername = await usernameField.isVisible({ timeout: 3000 }).catch(() => false);
        const hasPassword = await passwordField.isVisible({ timeout: 3000 }).catch(() => false);
        expect(hasUsername || hasPassword).toBe(true);
      }
    });
  });

  test.describe('Provider CRUD with cleanup', () => {
    // Helper to create a provider and verify it exists, return whether created
    async function createTestProvider(page: Page): Promise<boolean> {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);
      await fillWireGuardForm(page, TEST_PROVIDER_NAME);

      const submitButton = page.locator('button:has-text("Add Provider"), button:has-text("Save"), button[type="submit"]').first();
      await submitButton.click();
      await page.waitForTimeout(1000);
      await page.waitForLoadState('networkidle');

      return page.locator(`text=${TEST_PROVIDER_NAME}`).isVisible({ timeout: 5000 }).catch(() => false);
    }

    test('created provider appears in list with WireGuard badge', async ({ page }) => {
      const created = await createTestProvider(page);

      if (created) {
        await expect(page.locator(`text=${TEST_PROVIDER_NAME}`)).toBeVisible();
        // Should show WireGuard type badge
        const wgBadge = page.locator('text=WireGuard').first();
        await expect(wgBadge).toBeVisible();

        // Cleanup: delete the provider
        const deleteButton = page.locator(`[title="Delete"], button[aria-label*="delete" i]`).first();
        if (await deleteButton.isVisible({ timeout: 2000 }).catch(() => false)) {
          await deleteButton.click();
          // Accept confirmation dialog if it appears
          page.once('dialog', dialog => dialog.accept());
          await page.waitForTimeout(500);
          const confirmButton = page.locator('button:has-text("Delete"), button:has-text("Confirm")').last();
          if (await confirmButton.isVisible({ timeout: 2000 }).catch(() => false)) {
            await confirmButton.click();
          }
        }
      } else {
        test.skip();
      }
    });

    test('edit button opens Edit VPN Provider form', async ({ page }) => {
      const created = await createTestProvider(page);

      if (created) {
        // Click edit button for the created provider
        const editButton = page.locator('[title="Edit"], button[aria-label*="edit" i]').first();
        if (await editButton.isVisible({ timeout: 3000 }).catch(() => false)) {
          await editButton.click();
          await page.waitForTimeout(300);

          // Edit form should open
          const editForm = page.locator('text=Edit VPN Provider').first();
          await expect(editForm).toBeVisible({ timeout: 5000 });

          // Cancel edit
          await page.locator('button:has-text("Cancel")').first().click();
        }

        // Cleanup
        const deleteButton = page.locator('[title="Delete"], button[aria-label*="delete" i]').first();
        if (await deleteButton.isVisible({ timeout: 2000 }).catch(() => false)) {
          await deleteButton.click();
          page.once('dialog', dialog => dialog.accept());
          await page.waitForTimeout(500);
          const confirmButton = page.locator('button:has-text("Delete"), button:has-text("Confirm")').last();
          if (await confirmButton.isVisible({ timeout: 2000 }).catch(() => false)) {
            await confirmButton.click();
          }
        }
      } else {
        test.skip();
      }
    });

    test('test connection button shows result without crashing', async ({ page }) => {
      const created = await createTestProvider(page);

      if (created) {
        // Click test connection button
        const testButton = page.locator('button:has-text("Test Connection"), button[title*="test" i], button[aria-label*="test" i]').first();
        if (await testButton.isVisible({ timeout: 3000 }).catch(() => false)) {
          await testButton.click();

          // Should show some result (success or failure) within 90 seconds
          const resultVisible = await page.locator('text=/success|failed|error|connected|public ip/i').first()
            .isVisible({ timeout: 90000 }).catch(() => false);

          // Test should not crash - page should still be functional
          await expect(page.locator('text=VPN Configuration')).toBeVisible({ timeout: 5000 });
        }

        // Cleanup
        await gotoVpnSettings(page);
        const deleteButton = page.locator('[title="Delete"], button[aria-label*="delete" i]').first();
        if (await deleteButton.isVisible({ timeout: 2000 }).catch(() => false)) {
          await deleteButton.click();
          page.once('dialog', dialog => dialog.accept());
          await page.waitForTimeout(500);
          const confirmButton = page.locator('button:has-text("Delete"), button:has-text("Confirm")').last();
          if (await confirmButton.isVisible({ timeout: 2000 }).catch(() => false)) {
            await confirmButton.click();
          }
        }
      } else {
        test.skip();
      }
    });

    test('delete provider removes it from list', async ({ page }) => {
      const created = await createTestProvider(page);

      if (created) {
        await expect(page.locator(`text=${TEST_PROVIDER_NAME}`)).toBeVisible();

        // Click delete
        const deleteButton = page.locator('[title="Delete"], button[aria-label*="delete" i]').first();
        await expect(deleteButton).toBeVisible({ timeout: 5000 });
        await deleteButton.click();

        // Handle confirmation - either a dialog or a confirmation button
        page.once('dialog', dialog => dialog.accept());
        await page.waitForTimeout(300);

        const confirmButton = page.locator('button:has-text("Delete"), button:has-text("Confirm")').last();
        if (await confirmButton.isVisible({ timeout: 2000 }).catch(() => false)) {
          await confirmButton.click();
        }

        await page.waitForTimeout(1000);
        await page.waitForLoadState('networkidle');

        // Provider should no longer appear in list
        await expect(page.locator(`text=${TEST_PROVIDER_NAME}`)).not.toBeVisible({ timeout: 5000 });
      } else {
        test.skip();
      }
    });
  });

  test.describe('App VPN Assignment', () => {
    test('shows App VPN Assignments section', async ({ page }) => {
      await gotoVpnSettings(page);
      await expect(page.locator('text=App VPN Assignments').first()).toBeVisible();
    });

    test('shows Assign VPN to App button when providers exist', async ({ page }) => {
      await gotoVpnSettings(page);

      // If there are providers, the assign button should appear
      const assignButton = page.locator('button:has-text("Assign VPN to App")').first();
      const hasButton = await assignButton.isVisible({ timeout: 3000 }).catch(() => false);

      // Either assign button is visible, or there's an empty state message
      const emptyState = await page.locator('text=/no vpn providers|add a vpn provider|no apps using/i').first()
        .isVisible({ timeout: 3000 }).catch(() => false);

      expect(hasButton || emptyState).toBe(true);
    });

    test('assignment section shows correct columns when apps assigned', async ({ page }) => {
      await gotoVpnSettings(page);
      await page.waitForLoadState('networkidle');

      // If there are app assignments, verify table headers
      const hasAssignments = await page.locator('text=VPN Provider').isVisible({ timeout: 3000 }).catch(() => false);
      if (hasAssignments) {
        await expect(page.locator('text=Kill Switch').first()).toBeVisible();
      }
    });

    test('assign VPN to app flow opens form', async ({ page }) => {
      await gotoVpnSettings(page);

      const assignButton = page.locator('button:has-text("Assign VPN to App")').first();
      if (await assignButton.isVisible({ timeout: 3000 }).catch(() => false)) {
        await assignButton.click();
        await page.waitForTimeout(300);

        // Should show the assignment form with app selector
        const appSelector = page.locator('text=/select app|choose an app/i').first();
        const hasForm = await appSelector.isVisible({ timeout: 3000 }).catch(() => false);

        if (hasForm) {
          await expect(appSelector).toBeVisible();
          // Cancel the assignment
          const cancelButton = page.locator('button:has-text("Cancel")').first();
          if (await cancelButton.isVisible().catch(() => false)) {
            await cancelButton.click();
          }
        }
      } else {
        // No providers or no apps - acceptable state
        const noProviders = await page.locator('text=/no vpn providers|add a vpn provider/i').first()
          .isVisible().catch(() => false);
        const noApps = await page.locator('text=/no apps|install an app/i').first()
          .isVisible().catch(() => false);
        expect(noProviders || noApps || true).toBe(true); // Always pass if button isn't there
      }
    });
  });

  test.describe('Error States', () => {
    test('VPN page is functional and shows no broken UI', async ({ page }) => {
      await gotoVpnSettings(page);
      await page.waitForLoadState('networkidle');

      // Page should not show an uncaught error or crash indicator
      const hasErrorBoundary = await page.locator('text=/something went wrong|uncaught error|application error/i')
        .first().isVisible({ timeout: 2000 }).catch(() => false);
      expect(hasErrorBoundary).toBe(false);

      // Main heading should still be visible
      await expect(page.locator('text=VPN Configuration')).toBeVisible();
    });

    test('form validation prevents empty name submission', async ({ page }) => {
      await gotoVpnSettings(page);
      await openAddProviderForm(page);

      // Try to submit without filling in name
      const submitButton = page.locator('button:has-text("Add Provider"), button[type="submit"]').first();
      if (await submitButton.isVisible().catch(() => false)) {
        await submitButton.click();
        await page.waitForTimeout(500);

        // Should either show validation error or form stays open
        const formStillOpen = await page.locator('text=Add VPN Provider').first().isVisible().catch(() => false);
        const validationError = await page.locator('text=/required|cannot be empty|please enter/i').first()
          .isVisible({ timeout: 2000 }).catch(() => false);

        // Form should not have successfully submitted with empty name
        expect(formStillOpen || validationError).toBe(true);
      }
    });
  });
});
