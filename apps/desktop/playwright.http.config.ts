import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./http-e2e", timeout: 45_000, fullyParallel: false, retries: 0, reporter: "list",
  use: { baseURL: process.env.ENTWURF_HTTP_TEST_URL ?? "http://127.0.0.1:1437", trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
