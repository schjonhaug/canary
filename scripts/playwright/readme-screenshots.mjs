import { chromium, expect } from "@playwright/test"
import { execFile } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { promisify } from "node:util"
import { fileURLToPath } from "node:url"

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(scriptDir, "../..")
const execFileAsync = promisify(execFile)

const frontendUrl = process.env.FRONTEND_URL || "http://localhost:3001"
const walletDescriptor =
  process.env.WALLET_DESCRIPTOR ||
  "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q"
let authToken = process.env.AUTH_TOKEN || ""
const ntfyTopic = process.env.NTFY_TOPIC || "canary-readme-screenshots"
const dashboardTransactionCount = Number(process.env.README_SCREENSHOT_TRANSACTION_COUNT || "25")
const metadataDbPath =
  process.env.CANARY_METADATA_DB_PATH ||
  path.join(repoRoot, "backend/database/self-hosted/regtest/metadata.sqlite")

const outputPaths = [
  process.env.README_SCREENSHOT_00 || path.join(repoRoot, "screenshots/screenshot-00.png"),
  process.env.README_SCREENSHOT_01 || path.join(repoRoot, "screenshots/screenshot-01.png"),
  process.env.README_SCREENSHOT_02 || path.join(repoRoot, "screenshots/screenshot-02.png"),
  process.env.README_SCREENSHOT_03 || path.join(repoRoot, "screenshots/screenshot-03.png"),
  process.env.README_SCREENSHOT_04 || path.join(repoRoot, "screenshots/screenshot-04.png"),
]

function apiUrl(endpoint) {
  return new URL(endpoint, frontendUrl).toString()
}

function authHeaders() {
  const headers = { "Content-Type": "application/json" }
  if (authToken) {
    headers.Authorization = `Bearer ${authToken}`
    headers.Cookie = `auth_token=${authToken}`
  }
  return headers
}

async function apiRequest(endpoint, options = {}) {
  const response = await fetch(apiUrl(endpoint), {
    ...options,
    headers: {
      ...authHeaders(),
      ...(options.headers || {}),
    },
  })

  if (!response.ok) {
    const body = await response.text()
    throw new Error(`${options.method || "GET"} ${endpoint} failed with ${response.status}: ${body}`)
  }

  if (response.status === 204) {
    return null
  }

  const body = await response.text()
  return parseJson(body) ?? body
}

async function authenticateIfRequired() {
  if (authToken) {
    return
  }

  const response = await fetch(apiUrl("/api/wallets"), {
    headers: authHeaders(),
  })

  if (response.status !== 401) {
    return
  }

  const login = await apiRequest("/api/auth/login", {
    method: "POST",
    body: JSON.stringify({
      email: process.env.CANARY_SELF_HOSTED_ADMIN_EMAIL || "admin@local",
      password:
        process.env.CANARY_SELF_HOSTED_ADMIN_PASSWORD ||
        "replace-with-a-strong-password",
    }),
  })

  authToken = login.token || ""
  if (!authToken) {
    throw new Error("Self-hosted login did not return an auth token")
  }
}

async function tryApiRequest(endpoint, options = {}) {
  const response = await fetch(apiUrl(endpoint), {
    ...options,
    headers: {
      ...authHeaders(),
      ...(options.headers || {}),
    },
  })

  const body = await response.text()
  return {
    ok: response.ok,
    status: response.status,
    body,
    json: parseJson(body),
  }
}

function parseJson(body) {
  if (!body) {
    return null
  }

  try {
    return JSON.parse(body)
  } catch {
    return null
  }
}

async function ensurePreferences() {
  await apiRequest("/api/user/preferences", {
    method: "PUT",
    body: JSON.stringify({
      preferred_language: "en-US",
      preferred_fiat_currency: "USD",
    }),
  })
}

async function resolveScreenshotWallet() {
  const walletsResponse = await apiRequest("/api/wallets")
  const existingByDescriptor = walletsResponse.wallets.find(
    (wallet) => wallet.descriptor === walletDescriptor
  )

  if (existingByDescriptor) {
    return existingByDescriptor
  }

  const created = await tryApiRequest("/api/wallets", {
    method: "POST",
    body: JSON.stringify({
      name: "Test",
      descriptor: walletDescriptor,
      preferred_language: "en-US",
    }),
  })

  if (created.ok) {
    return created.json.wallet
  }

  if (created.status === 409) {
    const match = created.body.match(/ID:\s*([A-Za-z0-9_-]+)/)
    if (match) {
      return {
        checksum: match[1],
        name: "Test",
        descriptor: walletDescriptor,
      }
    }
  }

  throw new Error(`Could not resolve screenshot wallet: ${created.status} ${created.body}`)
}

async function getWalletDetail(checksum) {
  return apiRequest(`/api/wallets/${checksum}/detail?page_size=100`)
}

async function ensureWalletName(wallet) {
  if (wallet.name === "Test") {
    return
  }

  await apiRequest(`/api/wallets/${wallet.checksum}`, {
    method: "PUT",
    body: JSON.stringify({ name: "Test" }),
  })
}

async function waitForWalletData(checksum) {
  const deadline = Date.now() + 180_000
  let lastDetail = null

  while (Date.now() < deadline) {
    lastDetail = await getWalletDetail(checksum)
    if ((lastDetail.wallet.balance_total || 0) > 0 && lastDetail.transactions.length > 0) {
      return lastDetail
    }
    await new Promise((resolve) => setTimeout(resolve, 2_000))
  }

  const balance = lastDetail?.wallet.balance_total || 0
  const transactionCount = lastDetail?.transactions.length || 0
  throw new Error(
    `Timed out waiting for screenshot wallet data: balance=${balance}, transactions=${transactionCount}`
  )
}

async function ensureNtfyContact(checksum, detail) {
  for (const contact of detail.contacts) {
    const isTargetContact =
      contact.name === "John" &&
      contact.notification_methods?.some(
        (method) =>
          method.provider_type === "ntfy" &&
          method.notification_target === ntfyTopic
      )
    if (!isTargetContact) {
      await apiRequest(`/api/wallets/${checksum}/contacts/${contact.id}`, {
        method: "DELETE",
      })
    }
  }

  detail = await getWalletDetail(checksum)
  const existing = detail.contacts.some(
    (contact) =>
      contact.name === "John" &&
      contact.notification_methods?.some(
        (method) =>
          method.provider_type === "ntfy" &&
          method.notification_target === ntfyTopic
      )
  )

  if (existing) {
    return
  }

  await apiRequest(`/api/wallets/${checksum}/contacts`, {
    method: "POST",
    body: JSON.stringify({
      name: "John",
      notification_methods: [
        {
          provider_type: "ntfy",
          notification_target: ntfyTopic,
        },
      ],
    }),
  })
}

async function removeStaleScreenshotNotificationHistory(checksum) {
  if (!fs.existsSync(metadataDbPath)) {
    return
  }

  await execFileAsync("sqlite3", [
    metadataDbPath,
    `DELETE FROM notification_logs
     WHERE transaction_wallet_checksum = '${checksum.replaceAll("'", "''")}'
       AND contact_name_snapshot IS NOT NULL
       AND contact_name_snapshot <> 'John';`,
  ])
}

async function ensureBalanceAlert(checksum, detail, alertData) {
  const duplicate = detail.balance_alerts.some((alert) => {
    if (alert.alert_type !== alertData.alert_type) {
      return false
    }
    if (alertData.threshold_sats !== undefined) {
      return alert.threshold_sats === alertData.threshold_sats && !alert.threshold_currency
    }
    return (
      alert.threshold_currency === alertData.threshold_currency &&
      Number(alert.threshold_fiat_amount) === Number(alertData.threshold_fiat_amount)
    )
  })

  if (duplicate) {
    return
  }

  const result = await tryApiRequest(`/api/wallets/${checksum}/balance-alerts`, {
    method: "POST",
    body: JSON.stringify(alertData),
  })

  if (!result.ok && result.status !== 409) {
    throw new Error(`Could not create balance alert: ${result.status} ${result.body}`)
  }
}

async function ensureBalanceAlerts(checksum, detail) {
  for (const alert of detail.balance_alerts) {
    await apiRequest(`/api/balance-alerts/${alert.id}`, {
      method: "DELETE",
    })
  }

  detail = await getWalletDetail(checksum)
  await ensureBalanceAlert(checksum, detail, {
    alert_type: "below",
    threshold_sats: 50_000_000,
  })

  detail = await getWalletDetail(checksum)
  await ensureBalanceAlert(checksum, detail, {
    alert_type: "above",
    threshold_currency: "USD",
    threshold_fiat_amount: 1_000_000,
  })

  detail = await getWalletDetail(checksum)
  await ensureBalanceAlert(checksum, detail, {
    alert_type: "equals",
    threshold_sats: 0,
  })
}

async function prepareFixture() {
  await ensurePreferences()
  const wallet = await resolveScreenshotWallet()
  await ensureWalletName(wallet)
  let detail = await waitForWalletData(wallet.checksum)
  await ensureNtfyContact(wallet.checksum, detail)
  await removeStaleScreenshotNotificationHistory(wallet.checksum)
  detail = await getWalletDetail(wallet.checksum)
  await ensureBalanceAlerts(wallet.checksum, detail)
  return wallet.checksum
}

async function waitForDashboardReady(checksum) {
  const deadline = Date.now() + 180_000
  let lastDetail = null

  while (Date.now() < deadline) {
    lastDetail = await getWalletDetail(checksum)
    const hasPending = lastDetail.transactions.some(
      (transaction) => transaction.block_height === null
    )
    if (lastDetail.transactions.length >= dashboardTransactionCount && hasPending) {
      return
    }
    await new Promise((resolve) => setTimeout(resolve, 2_000))
  }

  const transactionCount = lastDetail?.transactions.length || 0
  const pendingCount =
    lastDetail?.transactions.filter((transaction) => transaction.block_height === null).length || 0
  throw new Error(
    `Timed out waiting for ${dashboardTransactionCount} transactions with a pending transaction: transactions=${transactionCount}, pending=${pendingCount}`
  )
}

function compactDashboardDetail(detail) {
  const transactions = [...detail.transactions]
  const pendingIndex = transactions.findIndex((transaction) => transaction.block_height === null)

  if (pendingIndex > -1) {
    const [pendingTransaction] = transactions.splice(pendingIndex, 1)
    transactions.unshift(pendingTransaction)
  }

  return {
    ...detail,
    transactions: transactions.slice(0, dashboardTransactionCount),
    pagination: {
      ...detail.pagination,
      has_more: false,
      next_cursor: null,
    },
  }
}

async function useCompactDashboardDetail(page, checksum) {
  await page.route(`**/api/wallets/${checksum}/detail**`, async (route) => {
    const response = await route.fetch()
    const detail = await response.json()
    await route.fulfill({
      response,
      json: compactDashboardDetail(detail),
    })
  })
}

async function attachAuthCookie(context) {
  const cookies = [
    {
      name: "locale",
      value: "en-US",
      url: frontendUrl,
      sameSite: "Lax",
    },
  ]

  if (authToken) {
    cookies.push({
      name: "auth_token",
      value: authToken,
      url: frontendUrl,
      httpOnly: true,
      sameSite: "Lax",
    })
  }

  await context.addCookies(cookies)
}

async function hideDevelopmentChrome(context) {
  await context.addInitScript(() => {
    const style = document.createElement("style")
    style.textContent = `
      nextjs-portal,
      [data-nextjs-toast],
      [data-nextjs-dialog-overlay],
      [data-nextjs-devtools-button],
      [aria-label="Open Next.js Dev Tools"] {
        display: none !important;
      }
    `
    document.documentElement.appendChild(style)
  })
}

async function capture(page, outputPath) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true })
  await page.evaluate(() => {
    document
      .querySelectorAll("nextjs-portal, [data-nextjs-dev-overlay]")
      .forEach((element) => element.remove())
  })
  await page.screenshot({
    path: outputPath,
    fullPage: true,
  })
  assertPngWidth(outputPath, 2560)
  console.log(`Wrote ${path.basename(outputPath)}`)
}

function assertPngWidth(outputPath, expectedWidth) {
  const header = fs.readFileSync(outputPath).subarray(0, 24)
  const width = header.readUInt32BE(16)
  if (width !== expectedWidth) {
    throw new Error(`${path.basename(outputPath)} is ${width}px wide, expected ${expectedWidth}px`)
  }
}

async function captureEmptyWallets(page) {
  await page.route("**/api/wallets", async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ timestamp: Date.now(), wallets: [] }),
      })
      return
    }
    await route.continue()
  })

  await page.goto("/wallets")
  await expect(page.getByText(/no wallets|add wallet/i).first()).toBeVisible()
  await capture(page, outputPaths[0])
  await page.unroute("**/api/wallets")
}

async function captureAddWallet(page) {
  await page.goto("/wallets/add")
  await expect(page.getByLabel(/wallet name/i)).toBeVisible()
  await page.getByLabel(/wallet name/i).fill("Test")
  await page.getByLabel(/output descriptor|xpub|address/i).fill(walletDescriptor)
  await capture(page, outputPaths[1])
}

async function captureSparrowGuide(page) {
  await page.goto("/wallets/add/sparrow")
  await expect(page.getByText("Sparrow", { exact: true }).first()).toBeVisible()
  await expect(page.getByText(/export/i).first()).toBeVisible()
  await capture(page, outputPaths[2])
}

async function captureWalletDashboard(page, checksum) {
  await waitForDashboardReady(checksum)
  await useCompactDashboardDetail(page, checksum)
  await page.goto(`/wallets/${checksum}`)
  await expect(page.getByText(/Balance/i).first()).toBeVisible()
  await expect(page.getByText(/Transactions/i).first()).toBeVisible()
  await expect(page.getByText(`${dashboardTransactionCount} transactions`)).toBeVisible()
  await expect(
    page.locator("tbody tr").filter({ hasText: /Sending|Receiving/i }).first()
  ).toBeVisible()
  await expect(page.getByText(ntfyTopic, { exact: true })).toBeVisible()
  await expect(page.getByText(/Below/i).first()).toBeVisible()
  await expect(page.getByText(/Above/i).first()).toBeVisible()
  await expect(page.getByText(/Equals/i).first()).toBeVisible()

  const transactionRow = page.locator("tbody tr").filter({ hasText: /Sent|Received/i }).first()
  await transactionRow.getByRole("button", { name: /expand transaction details/i }).click()
  await expect(page.locator('[id^="transaction-details-"]').first()).toBeVisible()

  const walletDetails = page.getByRole("button", { name: /Wallet Details/i })
  await walletDetails.click()
  await expect(page.getByText(/Output Descriptor|Address/i).first()).toBeVisible()
  await capture(page, outputPaths[3])
  await page.unroute(`**/api/wallets/${checksum}/detail**`)
}

async function captureSettings(page) {
  await page.goto("/settings")
  await expect(page.getByText(/English/i).first()).toBeVisible()
  await expect(page.getByText(/USD/i).first()).toBeVisible()
  await capture(page, outputPaths[4])
}

async function main() {
  await authenticateIfRequired()
  const checksum = await prepareFixture()

  const browser = await chromium.launch()
  const context = await browser.newContext({
    baseURL: frontendUrl,
    viewport: { width: 1280, height: 900 },
    deviceScaleFactor: 2,
  })

  try {
    await attachAuthCookie(context)
    await hideDevelopmentChrome(context)
    const page = await context.newPage()
    await captureEmptyWallets(page)
    await captureAddWallet(page)
    await captureSparrowGuide(page)
    await captureWalletDashboard(page, checksum)
    await captureSettings(page)
  } finally {
    await browser.close()
  }
}

await main()
