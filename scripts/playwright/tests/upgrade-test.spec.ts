import { expect, test, type Page } from "@playwright/test"

const walletChecksum = process.env.WALLET_CHECKSUM || ""
const walletName = process.env.WALLET_NAME || ""
const ntfyTopic = process.env.NTFY_TOPIC || ""
const authToken = process.env.AUTH_TOKEN
const expectedWalletCount = Number(process.env.EXPECTED_WALLET_COUNT || "0")
const txidPrefix = process.env.TXID_PREFIX
const walletLink = `a[href^="/wallets/${walletChecksum}"]`

test.skip(
  !walletChecksum || !walletName || !ntfyTopic,
  "WALLET_CHECKSUM, WALLET_NAME, and NTFY_TOPIC must be set"
)

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
  await expect(page.locator(walletLink)).toBeVisible()
  await page.locator(walletLink).click()
  await expect(page).toHaveURL(new RegExp(`/wallets/${walletChecksum}(?:/transactions)?$`))

  if (page.url().endsWith("/transactions")) {
    await page.goto(`/wallets/${walletChecksum}/notifications`)
    await expect(page).toHaveURL(new RegExp(`/wallets/${walletChecksum}/notifications$`))
  }
}

async function expectNtfyTopicVisible(page: Page) {
  await expect(page.getByText(ntfyTopic, { exact: false }).first()).toBeVisible()
}

async function expectTransactionsAvailable(page: Page) {
  const response = await page.request.get(`/api/wallets/${walletChecksum}/detail`)
  expect(response.ok()).toBe(true)

  const detail = await response.json()
  expect(detail.transactions.length).toBeGreaterThan(0)

  if (txidPrefix) {
    expect(
      detail.transactions.some((transaction: { txid: string }) =>
        transaction.txid.startsWith(txidPrefix)
      )
    ).toBe(true)
  }
}

test("@pre-upgrade wallet page shows the seeded wallet", async ({ page }) => {
  await page.goto("/wallets")
  await expect(page.locator(walletLink)).toContainText(walletName)
  if (expectedWalletCount > 0) {
    await expect.poll(async () => page.locator('a[href^="/wallets/"]').count())
      .toBeGreaterThanOrEqual(expectedWalletCount)
  }
})

test("@pre-upgrade wallet detail shows contact and transactions", async ({ page }) => {
  await openWalletDetail(page)
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
  await expectNtfyTopicVisible(page)
  await expectTransactionsAvailable(page)
})

test("@post-upgrade wallet page still shows the seeded wallet", async ({ page }) => {
  await page.goto("/wallets")
  await expect(page.locator(walletLink)).toContainText(walletName)
  if (expectedWalletCount > 0) {
    await expect.poll(async () => page.locator('a[href^="/wallets/"]').count())
      .toBeGreaterThanOrEqual(expectedWalletCount)
  }
})

test("@post-upgrade wallet detail still shows contact and transactions", async ({ page }) => {
  await openWalletDetail(page)
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
  await expectNtfyTopicVisible(page)
  await expectTransactionsAvailable(page)
})
