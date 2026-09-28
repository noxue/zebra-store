import { defineConfig, devices } from '@playwright/test'

/**
 * Base URLs come from the environment so the suite can run against any deployment:
 *   E2E_ADMIN_URL       admin SPA dev server        (default http://localhost:5186)
 *   E2E_STOREFRONT_URL  storefront SPA dev server   (default http://localhost:5185)
 *   E2E_API_URL         Rust backend (direct)       (default http://localhost:8082)
 *
 * The specs form one ordered business story (admin setup → guest purchase → member purchase →
 * admin follow-up) sharing state through `.state/state.json`, so they run serially in one worker.
 */
export default defineConfig({
  testDir: './tests',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 300_000,
  expect: { timeout: 30_000 },
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],
  outputDir: 'test-results',
  use: {
    ...devices['Desktop Chrome'],
    viewport: { width: 1440, height: 900 },
    locale: 'en-US',
    actionTimeout: 30_000,
    navigationTimeout: 60_000,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
})
