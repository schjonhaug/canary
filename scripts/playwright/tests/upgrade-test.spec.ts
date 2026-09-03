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
const selfHostedPassword = process.env.SELF_HOSTED_ADMIN_PASSWORD || ""
const expectedWalletCount = Number(process.env.EXPECTED_WALLET_COUNT || "0")
const txidPrefix = process.env.TXID_PREFIX
const walletLink = `a[href^="/wallets/${walletChecksum}"]`

test.skip(
  !walletChecksum || !walletName || !selfHostedPassword || ntfyTopics.some((topic) => !topic) || contactNames.some((name) => !name),
  "Wallet, contact, ntfy topic, and self-hosted password environment variables must be set"
)

test.beforeEach(async ({ page, context, baseURL }) => {
  expect((await context.cookies()).some((cookie) => cookie.name === "auth_token")).toBe(false)
  expect(baseURL).toBeTruthy()

  await page.goto("/sign-in")
  const password = page.getByLabel("Password")
  await expect(password).toBeVisible()
  await password.fill(selfHostedPassword)

  const loginRequestPromise = page.waitForRequest((request) =>
    request.method() === "POST" && request.url().endsWith("/api/auth/login")
  )
  const loginResponsePromise = page.waitForResponse((response) =>
    response.request().method() === "POST" && response.url().endsWith("/api/auth/login")
  )
  await page.locator('button[type="submit"]').click()

  const [loginRequest, loginResponse] = await Promise.all([
    loginRequestPromise,
    loginResponsePromise,
  ])
  const loginHeaders = await loginRequest.allHeaders()
  expect(loginHeaders.origin).toBe(new URL(baseURL!).origin)
  expect(loginHeaders["sec-fetch-site"]).toBe("same-origin")
  expect(loginResponse.ok()).toBe(true)
  await expect(page).toHaveURL(/\/wallets$/)
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
    await expect(card).toContainText("ntfy:")
    await expect(card).toContainText(index < 2 ? "Active" : "Inactive")
    await expect(page.getByText(ntfyTopics[index], { exact: true })).toHaveCount(0)
  }

  await expect(page.locator("input")).toHaveCount(0)
  await expect(page.getByRole("checkbox")).toHaveCount(0)
  await expect(page.locator("button:has(svg.lucide-pencil)")).toHaveCount(3)
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

async function renameWalletAndRestore(page: Page) {
  const temporaryName = `${walletName} auth gate`

  await page.getByRole("button", { name: "Edit", exact: true }).first().click()
  const nameInput = page.locator("input").first()
  await expect(nameInput).toHaveValue(walletName)
  await nameInput.fill(temporaryName)
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText(temporaryName, { exact: true })).toBeVisible()

  await page.getByRole("button", { name: "Edit", exact: true }).first().click()
  await expect(nameInput).toHaveValue(temporaryName)
  await nameInput.fill(walletName)
  await page.getByRole("button", { name: "Save", exact: true }).click()
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
}

async function signOut(page: Page) {
  await page.getByRole("button", { name: /Admin/ }).click()
  await page.getByRole("menuitem", { name: "Sign out" }).click()
  await expect(page).toHaveURL(/\/sign-in$/)
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
  await renameWalletAndRestore(page)
  await signOut(page)
})

test("@post-restart signs in again with preserved wallet and contacts", async ({ page }) => {
  await openWalletDetail(page)
  await expect(page.getByText(walletName, { exact: true })).toBeVisible()
  await expectMigratedCompactSummaries(page)
})
