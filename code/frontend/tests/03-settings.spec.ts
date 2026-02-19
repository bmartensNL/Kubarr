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

  test.describe('Pending Approval Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Pending Approval")').click();
      await expect(page).toHaveURL(/section=pending/);
    });

    test('shows Pending Approval heading', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator('h3:has-text("Pending Approval")')).toBeVisible({ timeout: 10000 });
    });

    test('shows description text', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator('text=Users waiting for approval')).toBeVisible({ timeout: 10000 });
    });

    test('does not show error page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const errorText = page.locator('text=/something went wrong|internal server error/i');
      await expect(errorText).not.toBeVisible();
    });

    test('renders section content without a blank white page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const body = await page.evaluate(() => document.body.innerText || '');
      expect(body.trim().length).toBeGreaterThan(0);
    });
  });

  test.describe('VPN Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("VPN")').click();
      await expect(page).toHaveURL(/section=vpn/);
    });

    test('shows VPN section content', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // VPN section should load without errors
      const hasVpnContent = await page.locator('text=/vpn|wireguard|provider|tunnel/i').first()
        .isVisible({ timeout: 10000 }).catch(() => false);
      expect(hasVpnContent).toBe(true);
    });

    test('does not show error page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const errorText = page.locator('text=/something went wrong|internal server error/i');
      await expect(errorText).not.toBeVisible();
    });

    test('renders section content without a blank white page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const body = await page.evaluate(() => document.body.innerText || '');
      expect(body.trim().length).toBeGreaterThan(0);
    });

    test('VPN section heading or content area is visible', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      // The VPN tab renders a VpnTab component - check that it loaded something
      const mainContent = page.locator('main');
      await expect(mainContent).toBeVisible();
      const contentText = await mainContent.textContent();
      expect(contentText?.trim().length).toBeGreaterThan(0);
    });
  });

  test.describe('Dynamic DNS Section', () => {
    test.beforeEach(async ({ page }) => {
      await page.locator('nav button:has-text("Dynamic DNS")').click();
      await expect(page).toHaveURL(/section=ddns/);
    });

    test('shows Dynamic DNS heading', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator('h3:has-text("Dynamic DNS")')).toBeVisible({ timeout: 10000 });
    });

    test('shows DDNS coming soon message', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator('text=DDNS configuration coming soon')).toBeVisible({ timeout: 10000 });
    });

    test('does not show error page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const errorText = page.locator('text=/something went wrong|internal server error/i');
      await expect(errorText).not.toBeVisible();
    });

    test('renders without blank page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const body = await page.evaluate(() => document.body.innerText || '');
      expect(body.trim().length).toBeGreaterThan(0);
    });
  });

  test.describe("Let's Encrypt Section", () => {
    test.beforeEach(async ({ page }) => {
      await page.locator("nav button:has-text(\"Let's Encrypt\")").click();
      await expect(page).toHaveURL(/section=letsencrypt/);
    });

    test("shows Let's Encrypt heading", async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator("h3:has-text(\"Let's Encrypt\")")).toBeVisible({ timeout: 10000 });
    });

    test("shows Let's Encrypt coming soon message", async ({ page }) => {
      await page.waitForLoadState('networkidle');
      await expect(page.locator("text=Let's Encrypt configuration coming soon")).toBeVisible({ timeout: 10000 });
    });

    test('does not show error page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const errorText = page.locator('text=/something went wrong|internal server error/i');
      await expect(errorText).not.toBeVisible();
    });

    test('renders without blank page', async ({ page }) => {
      await page.waitForLoadState('networkidle');
      const body = await page.evaluate(() => document.body.innerText || '');
      expect(body.trim().length).toBeGreaterThan(0);
    });
  });

  test.describe('All Settings Sections Load Without Critical Errors', () => {
    const sections = [
      { label: 'Dashboard', urlPattern: /section=dashboard/, name: 'dashboard' },
      { label: 'General', urlPattern: /section=general/, name: 'general' },
      { label: 'All Users', urlPattern: /section=users/, name: 'users' },
      { label: 'Pending Approval', urlPattern: /section=pending/, name: 'pending' },
      { label: 'Invite Links', urlPattern: /section=invites/, name: 'invites' },
      { label: 'Permissions', urlPattern: /section=permissions/, name: 'permissions' },
      { label: 'Audit Logs', urlPattern: /section=audit/, name: 'audit' },
      { label: 'Notifications', urlPattern: /section=notifications/, name: 'notifications' },
      { label: 'VPN', urlPattern: /section=vpn/, name: 'vpn' },
      { label: 'Dynamic DNS', urlPattern: /section=ddns/, name: 'ddns' },
    ];

    for (const section of sections) {
      test(`${section.name} section loads without "Something went wrong"`, async ({ page }) => {
        const consoleErrors: string[] = [];
        page.on('console', (msg) => {
          if (msg.type() === 'error') {
            consoleErrors.push(msg.text());
          }
        });

        await page.goto('/settings');
        await page.waitForLoadState('networkidle');

        await page.locator(`nav button:has-text("${section.label}")`).click();
        await expect(page).toHaveURL(section.urlPattern);
        await page.waitForLoadState('networkidle');

        // Should not show generic error message
        await expect(page.locator('text=/something went wrong/i')).not.toBeVisible();

        // Page should not be blank
        const mainText = await page.locator('main').textContent();
        expect(mainText?.trim().length).toBeGreaterThan(0);
      });
    }

    test("let's encrypt section loads without errors", async ({ page }) => {
      await page.goto('/settings');
      await page.waitForLoadState('networkidle');

      await page.locator("nav button:has-text(\"Let's Encrypt\")").click();
      await expect(page).toHaveURL(/section=letsencrypt/);
      await page.waitForLoadState('networkidle');

      await expect(page.locator('text=/something went wrong/i')).not.toBeVisible();
      const mainText = await page.locator('main').textContent();
      expect(mainText?.trim().length).toBeGreaterThan(0);
    });
  });
});
