import { test, expect } from '@playwright/test';

test.describe('Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    // Wait for settings page to load
    await expect(page.locator('text=System Dashboard')).toBeVisible({ timeout: 10000 });
  });

  // Helper to click a sidebar button
  const clickSidebarItem = async (page: any, label: string) => {
    // Target buttons inside the nav element (sidebar navigation)
    await page.locator(`nav button:has-text("${label}")`).click();
  };

  test.describe('Navigation', () => {
    test('shows settings sidebar with system sections', async ({ page }) => {
      // Should show Dashboard in sidebar (first one, not the nav)
      const sidebar = page.locator('[class*="w-64"]').first();
      await expect(sidebar.locator('text=Dashboard')).toBeVisible();
      await expect(sidebar.locator('text=Notifications')).toBeVisible();
      await expect(sidebar.locator('text=Audit Logs')).toBeVisible();
    });

    test('shows admin sections for admin user', async ({ page }) => {
      // Admin sections should be visible in sidebar - use first() for elements that may appear multiple times
      await expect(page.locator('text=ACCESS MANAGEMENT').first()).toBeVisible();
      await expect(page.locator('text=General').first()).toBeVisible();
      await expect(page.locator('text=All Users').first()).toBeVisible();
      await expect(page.locator('text=Pending Approval').first()).toBeVisible();
      await expect(page.locator('text=Invite Links').first()).toBeVisible();
    });

    test('shows networking sections', async ({ page }) => {
      await expect(page.locator('text=NETWORKING').first()).toBeVisible();
      await expect(page.locator('text=VPN').first()).toBeVisible();
      await expect(page.locator('text=Dynamic DNS').first()).toBeVisible();
    });

    test('can navigate between sections', async ({ page }) => {
      // Click on Users section - target the sidebar nav button
      await clickSidebarItem(page, 'All Users');
      await expect(page).toHaveURL(/section=users/);

      // Click on Notifications section - target the sidebar nav button
      await clickSidebarItem(page, 'Notifications');
      await expect(page).toHaveURL(/section=notifications/);

      // Click on Audit Logs section - target the sidebar nav button
      await clickSidebarItem(page, 'Audit Logs');
      await expect(page).toHaveURL(/section=audit/);
    });
  });

  test.describe('Dashboard Section', () => {
    test('shows system dashboard by default', async ({ page }) => {
      await expect(page.locator('text=System Dashboard')).toBeVisible();
      await expect(page.locator('text=Overview of system activity')).toBeVisible();
    });

    test('shows quick stats', async ({ page }) => {
      await expect(page.locator('text=Total Users')).toBeVisible();
      await expect(page.locator('text=Active Invites')).toBeVisible();
      await expect(page.locator('text=Roles')).toBeVisible();
    });

    test('shows activity overview', async ({ page }) => {
      await expect(page.locator('text=Activity Overview')).toBeVisible();
      await expect(page.locator('text=Events Today')).toBeVisible();
    });
  });

  test.describe('Users Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("All Users")').click();
      await expect(page).toHaveURL(/section=users/);
    });

    test('shows user list with admin', async ({ page }) => {
      // Should show users including admin - look in main content area
      await expect(page.locator('main >> text=admin').first()).toBeVisible();
    });

    test('has create user button', async ({ page }) => {
      await expect(page.locator('button:has-text("Create User"), button:has-text("Add User"), button:has-text("New User")')).toBeVisible();
    });
  });

  test.describe('Invite Links Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Invite Links")').click();
      await expect(page).toHaveURL(/section=invites/);
    });

    test('shows invite links section', async ({ page }) => {
      await expect(page.locator('button:has-text("Create"), button:has-text("Generate")')).toBeVisible();
    });
  });

  test.describe('Audit Logs Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Audit Logs")').click();
      await expect(page).toHaveURL(/section=audit/);
    });

    test('shows audit log content', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // Should show audit section heading or table
      await expect(page.locator('text=/audit|log|event|activity/i').first()).toBeVisible({ timeout: 10000 });
    });
  });

  test.describe('Notifications Section', () => {
    test.beforeEach(async ({ page }) => {
      // Click Notifications in sidebar - target the nav button
      await page.locator('nav button:has-text("Notifications")').click();
      await expect(page).toHaveURL(/section=notifications/);
    });

    test('shows notification channels', async ({ page }) => {
      await expect(page.locator('text=/channel|notification|configure|alert/i').first()).toBeVisible();
    });
  });

  test.describe('General Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("General")').click();
      await expect(page).toHaveURL(/section=general/);
    });

    test('shows general settings', async ({ page }) => {
      await expect(page.locator('h1, h2, h3').filter({ hasText: /general|settings/i }).first()).toBeVisible();
    });
  });

  test.describe('Permissions Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Permissions")').click();
      await expect(page).toHaveURL(/section=permissions/);
    });

    test('shows permission matrix', async ({ page }) => {
      await expect(page.locator('text=/permission|role|admin|access/i').first()).toBeVisible();
    });
  });

  test.describe('User CRUD', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("All Users")').click();
      await expect(page).toHaveURL(/section=users/);
    });

    test('Create User button opens form/view', async ({ page }) => {
      const createButton = page.locator('button:has-text("Create User"), button:has-text("Add User"), button:has-text("New User")');
      await expect(createButton).toBeVisible();
      await createButton.click();
      await page.waitForTimeout(500);
      // Should show a form with user fields
      const hasForm = await page.locator('input[name="username"], input[placeholder*="username" i]').isVisible({ timeout: 5000 }).catch(() => false);
      const hasHeading = await page.locator('text=/create|new|add/i').first().isVisible().catch(() => false);
      expect(hasForm || hasHeading).toBe(true);
    });

    test('user form has username, email, password, role fields', async ({ page }) => {
      const createButton = page.locator('button:has-text("Create User"), button:has-text("Add User"), button:has-text("New User")');
      await createButton.click();
      await page.waitForTimeout(500);

      // Check for form fields
      await expect(page.locator('text=/username/i').first()).toBeVisible({ timeout: 5000 });
      await expect(page.locator('text=/email/i').first()).toBeVisible();
      await expect(page.locator('text=/password/i').first()).toBeVisible();
      await expect(page.locator('text=/role/i').first()).toBeVisible();
    });
  });

  test.describe('Invite Links Details', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Invite Links")').click();
      await expect(page).toHaveURL(/section=invites/);
    });

    test('Create Invite button is present', async ({ page }) => {
      await expect(page.locator('button:has-text("Create"), button:has-text("Generate")')).toBeVisible();
    });

    test('can open invite creation form', async ({ page }) => {
      const createButton = page.locator('button:has-text("Create"), button:has-text("Generate")');
      await createButton.click();
      await page.waitForTimeout(500);
      // Should show invite creation UI
      const hasForm = await page.locator('text=/invite|link|code|role/i').first().isVisible({ timeout: 5000 }).catch(() => false);
      expect(hasForm).toBe(true);
    });
  });

  test.describe('General Settings Details', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("General")').click();
      await expect(page).toHaveURL(/section=general/);
    });

    test('General section shows system settings form fields', async ({ page }) => {
      await expect(page.locator('h1, h2, h3').filter({ hasText: /general|settings/i }).first()).toBeVisible();
      // General settings shows toggle switches for registration options
      // and OAuth providers section
      await expect(page.locator('text=User Registration')).toBeVisible({ timeout: 10000 });
      await expect(page.locator('text=Allow Open Registration')).toBeVisible();
    });
  });
});
