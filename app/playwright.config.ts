// UI smoke tests: the frontend in Chromium against the dev bridge (the real
// backend, run natively). `npm run e2e` starts both servers, or reuses ones
// already running. See e2e/.
import { existsSync } from 'node:fs';
import { defineConfig } from '@playwright/test';

// Use a system Chromium when there is one, so `npx playwright install` isn't
// needed; CHROMIUM_PATH overrides.
const chromium =
  process.env.CHROMIUM_PATH ??
  ['/usr/lib/chromium/chromium', '/usr/bin/chromium', '/usr/bin/google-chrome'].find((p) => existsSync(p));

export default defineConfig({
  testDir: 'e2e',
  // The bridge holds a single session, so tests can't run in parallel.
  workers: 1,
  fullyParallel: false,
  timeout: 30_000,
  use: {
    baseURL: 'http://localhost:1420',
    viewport: { width: 1180, height: 800 },
    launchOptions: chromium ? { executablePath: chromium } : {},
    trace: 'retain-on-failure',
  },
  webServer: [
    {
      command: 'npm run bridge',
      port: 1421,
      reuseExistingServer: true,
      // The first run compiles the backend.
      timeout: 600_000,
    },
    { command: 'npm run dev', port: 1420, reuseExistingServer: true },
  ],
});
