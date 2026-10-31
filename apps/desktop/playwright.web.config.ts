import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e", testMatch: ["browser.spec.ts", "automation.spec.ts"], timeout: 120_000,
  fullyParallel: false, retries: 0, reporter: "list",
  use: { baseURL: "http://127.0.0.1:1435/Entwurf/", trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 1000 } } }],
  webServer: { command: "pnpm exec vite preview --host 127.0.0.1 --port 1435 --base /Entwurf/", url: "http://127.0.0.1:1435/Entwurf/", reuseExistingServer: false, timeout: 60_000 },
});
