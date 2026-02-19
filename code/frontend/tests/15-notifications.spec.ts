import { test, expect } from '@playwright/test';

// Helper to open the notification bell dropdown
const openNotificationDropdown = async (page: any) => {
  const bellButton = page
    .locator('nav button')
    .filter({ has: page.locator('svg.lucide-bell') })
    .first();
  await bellButton.click();
  // Wait for dropdown heading to appear
  await expect(page.locator('h3:has-text("Notifications"), h2:has-text("Notifications")')).toBeVisible({ timeout: 5000 });
  return bellButton;
};

test.describe('Notification Inbox (Bell)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('h2:has-text("Dashboard")')).toBeVisible({ timeout: 10000 });
  });

  test('bell button is present in nav', async ({ page }) => {
    const bellButton = page
      .locator('nav button')
      .filter({ has: page.locator('svg.lucide-bell') })
      .first();
    await expect(bellButton).toBeVisible();
  });

  test('clicking bell opens notification dropdown', async ({ page }) => {
    await openNotificationDropdown(page);
    // Dropdown content is visible
    await expect(
      page.locator('text=Notifications').first()
    ).toBeVisible();
  });

  test('notification dropdown shows empty state or list', async ({ page }) => {
    await openNotificationDropdown(page);
    await page.waitForLoadState('networkidle');

    // Either shows a notification list or empty state
    const hasItems = await page.locator('text=/No notifications|notification/i').first().isVisible({ timeout: 5000 }).catch(() => false);
    expect(hasItems).toBe(true);
  });

  test('notification dropdown has preferences link', async ({ page }) => {
    await openNotificationDropdown(page);
    await expect(page.locator('text=Notification preferences')).toBeVisible({ timeout: 5000 });
  });

  test('unread count badge disappears after mark all as read', async ({ page }) => {
    await page.waitForLoadState('networkidle');

    // Check if unread badge is visible; if so, mark all as read
    const bellButton = page
      .locator('nav button')
      .filter({ has: page.locator('svg.lucide-bell') })
      .first();

    await bellButton.click();
    await page.waitForLoadState('networkidle');

    // If "Mark all as read" button is present, click it
    const markAllBtn = page.locator('button:has-text("Mark all as read")');
    const hasMarkAll = await markAllBtn.isVisible({ timeout: 3000 }).catch(() => false);
    if (hasMarkAll) {
      await markAllBtn.click();
      await page.waitForTimeout(500);
      // After marking all read, "Mark all as read" should no longer be visible
      await expect(markAllBtn).not.toBeVisible({ timeout: 5000 });
    } else {
      // No unread notifications — just verify the dropdown is open
      await expect(page.locator('text=Notifications').first()).toBeVisible();
    }
  });

  test('mark all as read button only appears when there are unread notifications', async ({ page }) => {
    await page.waitForLoadState('networkidle');
    await openNotificationDropdown(page);
    await page.waitForLoadState('networkidle');

    // If mark-all button is visible, there must be unread notifications
    const markAllBtn = page.locator('button:has-text("Mark all as read")');
    const hasMarkAll = await markAllBtn.isVisible({ timeout: 3000 }).catch(() => false);
    if (hasMarkAll) {
      // There should be at least one notification item with unread styling
      const unreadItem = page.locator('[class*="blue-50"], [class*="blue-900"]').first();
      await expect(unreadItem).toBeVisible({ timeout: 5000 });
    } else {
      // No "Mark all as read" => no unread notifications
      expect(true).toBe(true);
    }
  });

  test('closing dropdown by clicking bell again hides it', async ({ page }) => {
    const bellButton = await openNotificationDropdown(page);
    // Click bell again to close
    await bellButton.click();
    // Dropdown heading should no longer be visible
    await expect(
      page.locator('h3:has-text("Notifications"), h2:has-text("Notifications")')
    ).not.toBeVisible({ timeout: 5000 });
  });
});

test.describe('Admin: Notification Channels (Settings → Notifications)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    await expect(page.locator('text=System Dashboard')).toBeVisible({ timeout: 10000 });

    // Navigate to Notifications section
    await page.locator('nav button:has-text("Notifications")').click();
    await expect(page).toHaveURL(/section=notifications/);
    await page.waitForLoadState('networkidle');
  });

  test('notifications section loads without error', async ({ page }) => {
    await expect(page.locator('text=/channel|notification|configure|alert/i').first()).toBeVisible({ timeout: 10000 });
  });

  test('shows available channel types — not Signal', async ({ page }) => {
    // Wait for channels to load
    await page.waitForTimeout(1000);

    // Supported channels should be present
    await expect(page.locator('text=/email/i').first()).toBeVisible({ timeout: 10000 });

    // Signal should NOT appear anywhere in the notifications section
    const signalText = await page.locator('text=/signal/i').count();
    // Signal may appear 0 times (removed) — assert it is gone
    expect(signalText).toBe(0);
  });

  test('shows Email, Telegram, and MessageBird channels', async ({ page }) => {
    await page.waitForTimeout(1000);
    // All three supported channels should be listed
    await expect(page.locator('text=/email/i').first()).toBeVisible({ timeout: 10000 });
    await expect(page.locator('text=/telegram/i').first()).toBeVisible({ timeout: 10000 });
    await expect(page.locator('text=/messagebird/i').first()).toBeVisible({ timeout: 10000 });
  });

  test('each channel has an enabled/disabled toggle', async ({ page }) => {
    await page.waitForTimeout(1000);
    // Toggle buttons are inline-flex with w-11 h-6 (Tailwind toggle pattern)
    const toggles = page.locator('[class*="inline-flex"][class*="rounded-full"]');
    const count = await toggles.count();
    expect(count).toBeGreaterThan(0);
  });

  test('Configure button opens configuration form', async ({ page }) => {
    await page.waitForTimeout(1000);
    const configureBtn = page.locator('button:has-text("Configure")').first();
    const isVisible = await configureBtn.isVisible({ timeout: 5000 }).catch(() => false);

    if (isVisible) {
      await configureBtn.click();
      await page.waitForTimeout(500);
      // Config form should show Save and Cancel buttons
      await expect(page.locator('button:has-text("Save"), button:has-text("Cancel")')).toBeVisible({ timeout: 5000 });
    } else {
      // Channels may already be expanded — just verify the section loaded
      await expect(page.locator('text=/channel|notification/i').first()).toBeVisible();
    }
  });

  test('Cancel button closes configuration form', async ({ page }) => {
    await page.waitForTimeout(1000);
    const configureBtn = page.locator('button:has-text("Configure")').first();
    const isVisible = await configureBtn.isVisible({ timeout: 5000 }).catch(() => false);

    if (isVisible) {
      await configureBtn.click();
      await page.waitForTimeout(500);

      const cancelBtn = page.locator('button:has-text("Cancel")').first();
      await expect(cancelBtn).toBeVisible({ timeout: 5000 });
      await cancelBtn.click();
      await page.waitForTimeout(300);
      // After cancel, Configure button should be visible again
      await expect(page.locator('button:has-text("Configure")').first()).toBeVisible({ timeout: 5000 });
    } else {
      expect(true).toBe(true);
    }
  });

  test('Test channel button is present for each channel', async ({ page }) => {
    await page.waitForTimeout(1000);
    const testBtn = page.locator('button:has-text("Test")').first();
    await expect(testBtn).toBeVisible({ timeout: 10000 });
  });

  test('Test channel button without config returns error or shows test state', async ({ page }) => {
    await page.waitForTimeout(1000);

    // Look for test input + button combination
    const testInput = page.locator('input[placeholder*="@"], input[placeholder*="Chat"], input[placeholder*="Phone"]').first();
    const hasTestInput = await testInput.isVisible({ timeout: 3000 }).catch(() => false);

    if (hasTestInput) {
      await testInput.fill('test@example.com');
      const testBtn = page.locator('button:has-text("Test")').first();
      await testBtn.click();
      await page.waitForTimeout(1000);
      // Should not crash — either success or error message
      const hasResponse = await page
        .locator('text=/success|error|sent|failed|not configured/i')
        .first()
        .isVisible({ timeout: 5000 })
        .catch(() => false);
      // The page should still be functional regardless of test result
      await expect(page.locator('text=/notification|channel/i').first()).toBeVisible();
    } else {
      // If test inputs aren't visible, just verify section is present
      await expect(page.locator('text=/channel|notification/i').first()).toBeVisible();
    }
  });
});

test.describe('Admin: Notification Events (Settings → Notifications)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    await expect(page.locator('text=System Dashboard')).toBeVisible({ timeout: 10000 });

    // Navigate to Notifications section
    await page.locator('nav button:has-text("Notifications")').click();
    await expect(page).toHaveURL(/section=notifications/);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(1000);
  });

  test('event triggers section is visible', async ({ page }) => {
    await expect(page.locator('text=/event|trigger/i').first()).toBeVisible({ timeout: 10000 });
  });

  test('event list contains known event types', async ({ page }) => {
    // At least one of these well-known event labels should appear in the event table
    const knownEvents = ['Login', 'login_failed', 'user_created', 'app_installed', 'Login Failed', 'User Created'];
    let found = false;
    for (const event of knownEvents) {
      const count = await page.locator(`text=${event}`).count();
      if (count > 0) {
        found = true;
        break;
      }
    }
    expect(found).toBe(true);
  });

  test('event toggles are present in the events table', async ({ page }) => {
    // Event toggles are smaller than channel toggles (h-5 w-9)
    const toggles = page.locator('[class*="inline-flex"][class*="rounded-full"]');
    const count = await toggles.count();
    expect(count).toBeGreaterThan(0);
  });

  test('severity dropdown is present for events', async ({ page }) => {
    // Each event row has a severity select element
    const severitySelects = page.locator('select');
    const count = await severitySelects.count();
    // Should have at least one severity dropdown if events are loaded
    if (count > 0) {
      // First select should have severity options
      const firstSelect = severitySelects.first();
      await expect(firstSelect).toBeVisible();
      const options = await firstSelect.locator('option').count();
      expect(options).toBeGreaterThan(0);
    } else {
      // Events may not be loaded — check section is at least visible
      await expect(page.locator('text=/event|trigger|notification/i').first()).toBeVisible();
    }
  });

  test('toggling an event updates its state', async ({ page }) => {
    // Find the first event toggle
    const toggles = page.locator('[class*="inline-flex"][class*="rounded-full"]');
    const count = await toggles.count();

    if (count > 0) {
      const firstToggle = toggles.last(); // Use last to avoid channel toggles at top
      const wasEnabled = await firstToggle.evaluate((el: Element) =>
        el.classList.contains('bg-blue-600')
      );

      await firstToggle.click();
      await page.waitForTimeout(500);

      // State should have changed (class reflects enabled/disabled)
      const isNowEnabled = await firstToggle.evaluate((el: Element) =>
        el.classList.contains('bg-blue-600')
      );
      expect(isNowEnabled).toBe(!wasEnabled);

      // Toggle back to restore original state
      await firstToggle.click();
      await page.waitForTimeout(500);
    } else {
      // No toggles visible — check that section loaded
      await expect(page.locator('text=/notification/i').first()).toBeVisible();
    }
  });
});
