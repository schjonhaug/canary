import { expect, test, type Page } from "@playwright/test"

const walletChecksum = process.env.WALLET_CHECKSUM
const walletName = process.env.WALLET_NAME
const ntfyTopic = process.env.NTFY_TOPIC
const authToken = process.env.AUTH_TOKEN

if (!walletChecksum || !walletName || !ntfyTopic) {
  throw new Error("WALLET_CHECKSUM, WALLET_NAME, and NTFY_TOPIC must be set")
}

test.beforeEach(async ({ context, baseURL }) => {
  if (!authToken || !baseURL) {
    return
  }

  await context.addCookies([
    {
      name: "auth_token",
      value: authToken,
      url: baseURL,
      httpOnly: true,
      sameSite: "Lax",
    },
  ])
})

async function openWalletDetail(page: Page) {
  await page.goto("/wallets")
  await expect(page.locator(`a[href="/wallets/${walletChecksum}"]`)).toBeVisible()
  await page.locator(`a[href="/wallets/${walletChecksum}"]`).click()
  await expect(page).toHaveURL(new RegExp(`/wallets/${walletChecksum}$`))
}

async function expectTransactionsAvailable(page: Page) {
  const response = await page.request.get(`/api/wallets/${walletChecksum}/detail`)
  expect(response.ok()).toBe(true)

  const detail = await response.json()
  expect(detail.transactions.length).toBeGreaterThan(0)
}

test("@pre-upgrade wallet page shows the seeded wallet", async ({ page }) => {
  await page.goto("/wallets")
  await expect(page.locator(`a[href="/wallets/${walletChecksum}"]`)).toContainText(walletName)
})

test("@pre-upgrade wallet detail shows contact and transactions", async ({ page }) => {
  await openWalletDetail(page)
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
  await expect(page.getByText(ntfyTopic, { exact: true })).toBeVisible()
  await expectTransactionsAvailable(page)
})

test("@post-upgrade wallet page still shows the seeded wallet", async ({ page }) => {
  await page.goto("/wallets")
  await expect(page.locator(`a[href="/wallets/${walletChecksum}"]`)).toContainText(walletName)
})

test("@post-upgrade wallet detail still shows contact and transactions", async ({ page }) => {
  await openWalletDetail(page)
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
  await expect(page.getByText(ntfyTopic, { exact: true })).toBeVisible()
  await expectTransactionsAvailable(page)
})
