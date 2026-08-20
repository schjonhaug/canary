import { defineConfig } from "@playwright/test"

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  use: {
    baseURL: process.env.FRONTEND_URL || "http://localhost:3001",
    browserName: "chromium",
    headless: true,
    trace: "retain-on-failure",
  },
})
