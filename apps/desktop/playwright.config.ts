import { defineConfig, devices } from "@playwright/test";

const previewPort = 4173;
const previewUrl = `http://127.0.0.1:${previewPort}`;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: previewUrl,
    trace: "retain-on-failure",
  },
  webServer: {
    command:
      "pnpm exec vite build && pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort",
    url: previewUrl,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
