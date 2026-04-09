import { defineConfig } from "@playwright/test"

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  retries: 0,
  timeout: 30_000,
  use: {
    baseURL: "http://localhost:3001",
    browserName: "chromium",
    headless: true,
    trace: "retain-on-failure",
  },
})
