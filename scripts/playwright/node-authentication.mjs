import { chromium, expect } from "@playwright/test"

const publicUrl = process.env.CANARY_NODE_URL
const password = process.env.CANARY_SELF_HOSTED_ADMIN_PASSWORD
const dashboardPassword = process.env.CANARY_UMBREL_DASHBOARD_PASSWORD
const configuredDashboardUrl = process.env.CANARY_UMBREL_DASHBOARD_URL
const platform = process.env.CANARY_NODE_PLATFORM || ""
const stage = process.env.CANARY_NODE_STAGE || "node-authentication"
const mutate = process.env.CANARY_NODE_MUTATE !== "0"
const expectedContacts = (process.env.CANARY_EXPECTED_CONTACT_NAMES || "")
  .split(",")
  .map((name) => name.trim())
  .filter(Boolean)

if (!publicUrl) {
  throw new Error("CANARY_NODE_URL is required")
}
if (!password) {
  throw new Error("CANARY_SELF_HOSTED_ADMIN_PASSWORD is required")
}

const normalizedUrl = new URL(publicUrl)
const browser = await chromium.launch({ headless: true })

async function signIn(page, context) {
  const inheritedAuthCookies = (await context.cookies()).filter(
    (cookie) => cookie.name === "auth_token"
  )
  if (inheritedAuthCookies.length > 0) {
    throw new Error("Fresh browser context unexpectedly inherited an auth_token cookie")
  }

  await page.goto(new URL("/sign-in", normalizedUrl).toString(), {
    waitUntil: "domcontentloaded",
  })
  const redirectedUrl = new URL(page.url())
  if (redirectedUrl.origin !== normalizedUrl.origin) {
    if (platform !== "umbrel") {
      throw new Error(
        `Unexpected cross-origin redirect from ${normalizedUrl.origin} to ${redirectedUrl.origin}`
      )
    }
    if (!dashboardPassword) {
      throw new Error("Umbrel dashboard authentication is required but CANARY_UMBREL_DASHBOARD_PASSWORD is missing")
    }

    const expectedDashboardUrl = configuredDashboardUrl
      ? new URL(configuredDashboardUrl)
      : new URL(`${normalizedUrl.protocol}//${normalizedUrl.hostname}:2000/`)
    if (
      redirectedUrl.origin !== expectedDashboardUrl.origin ||
      redirectedUrl.pathname !== "/" ||
      redirectedUrl.searchParams.get("origin") !== "host" ||
      redirectedUrl.searchParams.get("app") !== "canary"
    ) {
      throw new Error(
        `Refusing to enter the Umbrel dashboard password at unexpected URL ${redirectedUrl.toString()}`
      )
    }

    const dashboardInput = page.locator('input[type="password"]').first()
    await dashboardInput.waitFor({ state: "visible" })
    await dashboardInput.fill(dashboardPassword)
    await page.locator('button[type="submit"]').click()
    await page.waitForURL((url) => url.origin === normalizedUrl.origin, { waitUntil: "domcontentloaded" })
  }
  const passwordInput = page.getByLabel("Password")
  await passwordInput.waitFor({ state: "visible" })
  const submitButton = page.locator('button[type="submit"]')

  // The form is server-rendered with its submit button disabled. Retrying the
  // fill until React enables the button gives us a deterministic hydration
  // signal without relying on networkidle, which polling apps may never reach.
  await expect(async () => {
    await passwordInput.fill("")
    await passwordInput.fill(password)
    await expect(submitButton).toBeEnabled({ timeout: 500 })
  }).toPass({ timeout: 10_000 })

  const loginRequestPromise = page.waitForRequest(
    (request) =>
      request.method() === "POST" &&
      new URL(request.url()).pathname === "/api/auth/login"
  )
  const loginResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/api/auth/login"
  )
  await submitButton.click()

  const [loginRequest, loginResponse] = await Promise.all([
    loginRequestPromise,
    loginResponsePromise,
  ])
  const headers = await loginRequest.allHeaders()
  if (headers.origin !== normalizedUrl.origin) {
    throw new Error(
      `Browser login Origin was ${headers.origin || "missing"}, expected ${normalizedUrl.origin}`
    )
  }
  const secFetchSite = headers["sec-fetch-site"]
  if (secFetchSite && secFetchSite !== "same-origin") {
    throw new Error(
      `Browser login Sec-Fetch-Site was ${secFetchSite}, expected same-origin`
    )
  }
  if (!secFetchSite && platform !== "umbrel") {
    throw new Error(
      "Browser login Sec-Fetch-Site was missing; only the Umbrel app proxy may omit it"
    )
  }
  if (!loginResponse.ok()) {
    throw new Error(`Browser login failed with HTTP ${loginResponse.status()}`)
  }

  await page.waitForURL(/\/wallets$/)
  return {
    origin: headers.origin,
    secFetchSite: secFetchSite || "missing",
  }
}

async function inspectWalletAndContacts(page) {
  await page.goto(new URL("/wallets", normalizedUrl).toString(), {
    waitUntil: "domcontentloaded",
  })
  const walletLinks = page.locator(
    'a[href^="/wallets/"]:not([href="/wallets/add"])'
  )
  await walletLinks.first().waitFor({ state: "visible" })
  const walletCount = await walletLinks.count()
  if (walletCount < 1) {
    throw new Error("Node authentication gate requires at least one preserved wallet")
  }

  await walletLinks.first().click()
  await page.waitForURL(/\/wallets\/[^/]+(?:\/transactions)?$/)
  const walletUrl = new URL(page.url())
  const walletPath = walletUrl.pathname.replace(/\/transactions$/, "")
  await page.goto(new URL(`${walletPath}/notifications`, normalizedUrl).toString(), {
    waitUntil: "domcontentloaded",
  })

  // The visible label is localized, while the edit icon is shared by every
  // supported locale. The wallet header uses lucide-square-pen instead.
  const contactEditButtons = page.locator("button:has(svg.lucide-pencil)")
  await contactEditButtons.first().waitFor({ state: "visible" })
  const contactCount = await contactEditButtons.count()
  if (contactCount < 1) {
    throw new Error("Node authentication gate requires at least one preserved contact")
  }
  for (const contactName of expectedContacts) {
    await page.getByText(contactName, { exact: true }).first().waitFor({ state: "visible" })
  }

  return { walletCount, contactCount, walletPath }
}

async function mutateWalletName(page, walletPath) {
  await page.goto(new URL(walletPath, normalizedUrl).toString(), {
    waitUntil: "domcontentloaded",
  })
  // Use stable icons here because the node can persist a non-English locale.
  const editButton = page.locator("button:has(svg.lucide-square-pen)").first()
  await editButton.waitFor({ state: "visible" })
  await editButton.click()

  const input = page.locator("input").first()
  const originalName = await input.inputValue()
  const temporaryName = `${originalName} auth gate`
  await input.fill(temporaryName)
  await page.locator("button:has(svg.lucide-check)").click()
  await page.getByText(temporaryName, { exact: true }).waitFor({ state: "visible" })

  await page.locator("button:has(svg.lucide-square-pen)").first().click()
  await input.waitFor({ state: "visible" })
  if ((await input.inputValue()) !== temporaryName) {
    throw new Error("Wallet name editor did not retain the temporary mutation")
  }
  await input.fill(originalName)
  await page.locator("button:has(svg.lucide-check)").click()
  await page.getByText(originalName, { exact: true }).waitFor({ state: "visible" })
}

async function signOut(page) {
  await page.locator("button:has(svg.lucide-user)").click()
  await page.locator('[role="menuitem"]:has(svg.lucide-log-out)').click()
  await page.waitForURL(/\/sign-in$/)
}

try {
  // A newly created Playwright context is incognito and has no shared cookie jar.
  const context = await browser.newContext({
    ignoreHTTPSErrors: true,
    locale: "en-US",
  })
  const page = await context.newPage()
  const provenance = await signIn(page, context)
  const inspected = await inspectWalletAndContacts(page)

  if (mutate) {
    await mutateWalletName(page, inspected.walletPath)
    await signOut(page)
  }

  console.log(
    JSON.stringify({
      stage,
      public_url: normalizedUrl.toString(),
      browser_authentication: "passed",
      natural_origin: provenance.origin,
      sec_fetch_site: provenance.secFetchSite,
      wallets_verified: inspected.walletCount,
      contacts_verified: inspected.contactCount,
      authenticated_mutation: mutate ? "wallet_name_round_trip" : "not_requested",
      signed_out: mutate,
    })
  )
  await context.close()
} finally {
  await browser.close()
}
