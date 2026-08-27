import { expect, test, type Page } from "@playwright/test"

const walletChecksum = process.env.WALLET_CHECKSUM || ""
const walletName = process.env.WALLET_NAME || ""
const ntfyTopics = [
  process.env.NTFY_TOPIC_A || "",
  process.env.NTFY_TOPIC_B || "",
  process.env.NTFY_TOPIC_INACTIVE || "",
]
const contactNames = [
  process.env.CONTACT_A_NAME || "",
  process.env.CONTACT_B_NAME || "",
  process.env.INACTIVE_CONTACT_NAME || "",
]
const authToken = process.env.AUTH_TOKEN
const expectedWalletCount = Number(process.env.EXPECTED_WALLET_COUNT || "0")
const txidPrefix = process.env.TXID_PREFIX
const walletLink = `a[href^="/wallets/${walletChecksum}"]`

test.skip(
  !walletChecksum || !walletName || ntfyTopics.some((topic) => !topic) || contactNames.some((name) => !name),
  "Wallet, contact, and ntfy topic environment variables must be set"
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

function redactedTopic(topic: string) {
  return topic.length <= 12
    ? `${topic.slice(0, 2)}••••${topic.slice(-2)}`
    : `${topic.slice(0, 7)}…${topic.slice(-5)}`
}

async function expectSourceContactsVisible(page: Page) {
  for (const name of contactNames) {
    await expect(page.getByText(name, { exact: true }).first()).toBeVisible()
  }
  for (const topic of ntfyTopics) {
    await expect(page.getByText(topic, { exact: false }).first()).toBeVisible()
  }
}

async function expectMigratedCompactSummaries(page: Page) {
  const response = await page.request.get(`/api/wallets/${walletChecksum}/detail`)
  expect(response.ok()).toBe(true)
  const detail = await response.json()

  for (const [index, name] of contactNames.entries()) {
    const contact = detail.contacts.find((candidate: { name: string }) => candidate.name === name)
    expect(contact).toBeTruthy()
    expect(contact.notification_methods).toEqual(expect.arrayContaining([
      expect.objectContaining({ provider_type: "ntfy", notification_target: ntfyTopics[index] }),
    ]))
    expect(contact.is_active).toBe(index < 2)

    const card = page.getByRole("heading", { name, exact: true }).locator("xpath=ancestor::*[@data-slot='card']")
    await expect(card).toBeVisible()
    await expect(card).toContainText(redactedTopic(ntfyTopics[index]))
    await expect(card).toContainText(index < 2 ? "Active" : "Inactive")
    await expect(page.getByText(ntfyTopics[index], { exact: true })).toHaveCount(0)
  }

  await expect(page.locator("input")).toHaveCount(0)
  await expect(page.getByRole("checkbox")).toHaveCount(0)
  await expect(page.getByRole("button", { name: /Edit contact/i })).toHaveCount(3)
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
  await expectSourceContactsVisible(page)
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
  await expectMigratedCompactSummaries(page)
  await expectTransactionsAvailable(page)
})
