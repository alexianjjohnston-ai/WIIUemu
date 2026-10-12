import { defineConfig, devices } from '@playwright/test';

const PORT = 4317;

// Chromium only (preinstalled; PLAYWRIGHT_BROWSERS_PATH points at it). The iPhone descriptor is used
// for its viewport / touch / DPR, rendered by Chromium.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: 'retain-on-failure',
    // Camera pointing: Chromium's built-in fake camera, permission pre-granted (localhost is a secure context).
    permissions: ['camera'],
    launchOptions: { args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] },
  },
  projects: [
    { name: 'pixel-7', use: { ...devices['Pixel 7'], browserName: 'chromium' } },
    {
      name: 'iphone-13',
      use: { ...devices['iPhone 13'], browserName: 'chromium' },
      testIgnore: /screenshots\.spec\.ts/,
    },
  ],
  webServer: {
    command: `npm run build && npx vite preview --port ${PORT} --strictPort`,
    url: `http://localhost:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
