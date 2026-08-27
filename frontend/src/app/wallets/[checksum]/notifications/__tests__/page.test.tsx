import React from "react"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import WalletNotificationsPage from "../page"
import { api } from "@/lib/api"
import { generateDraftId } from "@/components/wallet-notifications/utils"
import type {
  BalanceAlert,
  Contact,
  NotificationContentFields,
  Wallet,
  WalletNotificationsResponse,
} from "@/types"

const mockPush = jest.fn()
const mockSetCurrentWallet = jest.fn()
const mockUseAuth = jest.fn()
const mockNtfyTarget = jest.fn(() => ({
  url: "http://localhost:8080",
  defaultTopic: "managed-canary-topic",
  isBrowserSafe: true,
}))

Object.defineProperties(Element.prototype, {
  hasPointerCapture: { value: jest.fn(() => false) },
  setPointerCapture: { value: jest.fn() },
  releasePointerCapture: { value: jest.fn() },
  scrollIntoView: { value: jest.fn() },
})

Object.defineProperty(globalThis.crypto, "randomUUID", {
  configurable: true,
  value: jest.fn(() => "draft-alert-id"),
})

jest.mock("next/navigation", () => ({
  useParams: () => ({ checksum: "sq32h3ch" }),
  useRouter: () => ({ push: mockPush }),
}))

jest.mock("@/contexts/auth-context", () => ({ useAuth: () => mockUseAuth() }))
jest.mock("@/contexts/wallets-context", () => ({
  useWalletsContext: () => ({ setCurrentWallet: mockSetCurrentWallet }),
}))
jest.mock("@/components/wallet-detail", () => ({
  WalletDetailHeader: ({ walletName }: { walletName: string }) => <div>{walletName}</div>,
  WalletDetailSkeleton: () => <div>Loading wallet</div>,
  getWalletDetailErrorState: jest.fn(() => null),
}))
jest.mock("@/components/plans-modal", () => ({
  PlansModal: ({ isOpen }: { isOpen: boolean }) => isOpen ? <div data-testid="plans-modal">Upgrade</div> : null,
}))
jest.mock("@/hooks/useNtfyServerUrl", () => ({
  useNtfyServerTarget: () => mockNtfyTarget(),
}))
jest.mock("@/hooks/usePhonePlaceholder", () => ({ usePhonePlaceholder: () => "+47 123 45 678" }))

const verification = {
  verificationSent: false,
  verificationCode: "",
  verificationPhone: null as string | null,
  verificationAddress: null as string | null,
  isVerified: true,
  showSuccess: false,
  isSending: false,
  isVerifying: false,
  verificationError: null,
  phoneError: null,
  emailError: null,
  timeRemaining: 0,
  formatTime: jest.fn(() => "0:00"),
  setVerificationCode: jest.fn(),
  clearPhoneError: jest.fn(),
  clearEmailError: jest.fn(),
  clearVerificationError: jest.fn(),
  sendVerification: jest.fn(),
  verifyCode: jest.fn(),
  resendCode: jest.fn(),
  reset: jest.fn(),
}

jest.mock("@/hooks/useSmsVerification", () => ({ useSmsVerification: () => verification }))
jest.mock("@/hooks/useEmailVerification", () => ({ useEmailVerification: () => verification }))

jest.mock("@/lib/api", () => ({
  ApiError: class ApiError extends Error {
    errorCode = null
    getUserFriendlyMessage() { return this.message }
  },
  api: {
    getWalletNotifications: jest.fn(),
    getUserPreferences: jest.fn(),
    getProviders: jest.fn(),
    createContact: jest.fn(),
    updateContact: jest.fn(),
    deleteContact: jest.fn(),
    validateBalanceAlert: jest.fn(),
    createBalanceAlert: jest.fn(),
    deleteBalanceAlert: jest.fn(),
    sendTestNtfyNotification: jest.fn(),
    sendTestNostrNotification: jest.fn(),
    sendTestWebhookNotification: jest.fn(),
  },
}))

const mockApi = api as jest.Mocked<typeof api>

const USEFUL: NotificationContentFields = {
  wallet_name: true,
  event_type: true,
  transaction_amount: false,
  transaction_balance: false,
  balance_alert_condition: false,
  balance_alert_threshold: false,
  balance_alert_balance: false,
}
const PRIVATE: NotificationContentFields = {
  wallet_name: false,
  event_type: false,
  transaction_amount: false,
  transaction_balance: false,
  balance_alert_condition: false,
  balance_alert_threshold: false,
  balance_alert_balance: false,
}
const DETAILED: NotificationContentFields = {
  wallet_name: true,
  event_type: true,
  transaction_amount: true,
  transaction_balance: true,
  balance_alert_condition: true,
  balance_alert_threshold: true,
  balance_alert_balance: true,
}

const wallet: Wallet = {
  checksum: "sq32h3ch",
  name: "Regtest Wallet",
  descriptor: "wpkh([abcd/84h/1h/0h]tpub/0/*)",
  wallet_filename: "regtest-wallet",
  hex_color: "#f59e0b",
  created_at: "2024-01-01T00:00:00Z",
  balance_total: 50000000,
  last_activity: null,
  status: "ready",
  contact_count: 1,
  is_active: true,
  wallet_type: "descriptor",
}

function makeContact(overrides: Partial<Contact> = {}): Contact {
  const id = overrides.id ?? "contact-1"
  return {
    id,
    wallet_checksum: wallet.checksum,
    name: "Alice",
    notification_methods: [{
      id: `${id}-ntfy`,
      contact_id: id,
      provider_type: "ntfy",
      notification_target: "alice-private-topic",
      display_target: "alice-private-topic",
      created_at: "2024-01-01T00:00:00Z",
      is_enabled: true,
      content_fields: USEFUL,
    }],
    created_at: "2024-01-01T00:00:00Z",
    is_active: true,
    notify_sending: true,
    notify_sent: true,
    notify_receiving: true,
    notify_received: true,
    notify_cpfp: false,
    notify_rbf: false,
    include_wallet_balance_in_tx_notifications: false,
    ...overrides,
  }
}

function makeAlert(overrides: Partial<BalanceAlert> = {}): BalanceAlert {
  return {
    id: "alert-1",
    wallet_checksum: wallet.checksum,
    contact_id: "contact-1",
    threshold_sats: 100000000,
    alert_type: "above",
    is_active: true,
    created_at: "2024-01-01T00:00:00Z",
    ...overrides,
  }
}

function setResponse(contacts: Contact[] = [makeContact()], balanceAlerts: BalanceAlert[] = []) {
  const response: WalletNotificationsResponse = {
    timestamp: Date.now(), wallet, contacts, balance_alerts: balanceAlerts,
  }
  mockApi.getWalletNotifications.mockResolvedValue(response)
}

async function renderLoaded() {
  render(<WalletNotificationsPage />)
  await screen.findByRole("heading", { name: "Notifications" })
}

describe("WalletNotificationsPage", () => {
  beforeEach(() => {
    jest.clearAllMocks()
    jest.spyOn(window, "confirm").mockReturnValue(true)
    verification.isVerified = true
    verification.verificationPhone = null
    verification.verificationAddress = null
    mockNtfyTarget.mockReturnValue({
      url: "http://localhost:8080", defaultTopic: "managed-canary-topic", isBrowserSafe: true,
    })
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: false,
      isSelfHostedMode: true,
      user: { id: 1, email: "test@example.com" },
      billingStatus: null,
    })
    setResponse()
    mockApi.getUserPreferences.mockResolvedValue({ preferred_fiat_currency: "NOK" })
    mockApi.getProviders.mockResolvedValue({ providers: [
      { name: "ntfy", display_name: "ntfy", config_schema: {} },
      { name: "nostr", display_name: "Nostr", config_schema: {} },
      { name: "webhook", display_name: "Webhook", config_schema: {} },
    ] })
    mockApi.createContact.mockResolvedValue({ id: "created-contact" })
    mockApi.updateContact.mockResolvedValue(makeContact())
    mockApi.deleteContact.mockResolvedValue(undefined)
    mockApi.validateBalanceAlert.mockResolvedValue(undefined)
    mockApi.createBalanceAlert.mockResolvedValue(makeAlert())
    mockApi.deleteBalanceAlert.mockResolvedValue(undefined)
    mockApi.sendTestNtfyNotification.mockResolvedValue({ success: true })
    mockApi.sendTestNostrNotification.mockResolvedValue({ success: true, error: null })
    mockApi.sendTestWebhookNotification.mockResolvedValue({ success: true })
  })

  it("generates secure draft IDs when randomUUID is unavailable", () => {
    const randomUUID = globalThis.crypto.randomUUID
    Object.defineProperty(globalThis.crypto, "randomUUID", { configurable: true, value: undefined })
    try {
      expect(generateDraftId()).toMatch(/^draft-[0-9a-f]{32}$/)
    } finally {
      Object.defineProperty(globalThis.crypto, "randomUUID", { configurable: true, value: randomUUID })
    }
  })

  it("renders compact, sorted, redacted summaries without event or balance controls", async () => {
    setResponse([
      makeContact({ id: "z", name: "Zoe", notify_rbf: true }),
      makeContact({
        id: "a",
        name: "alice",
        notification_methods: [{
          id: "webhook", contact_id: "a", provider_type: "webhook",
          notification_target: "https://hooks.example.com/canary?secret=top-secret",
          created_at: "2024-01-01T00:00:00Z", is_enabled: true, content_fields: PRIVATE,
        }],
      }),
    ], [makeAlert({ id: "one", contact_id: "a" }), makeAlert({ id: "two", contact_id: "a", alert_type: "below" })])

    await renderLoaded()

    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual(["alice", "Zoe"])
    expect(screen.getByText("Webhook: https://hooks.example.com")).toBeInTheDocument()
    expect(screen.queryByText(/top-secret/)).not.toBeInTheDocument()
    expect(screen.getByText("Webhook: Private")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "alice" }).closest("[data-slot='card']")).toHaveTextContent("2 balance")
    expect(screen.getByText("Activity, confirmation, and advanced alerts")).toBeInTheDocument()
    expect(screen.queryByRole("checkbox", { name: "Activity detected" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Add alert" })).not.toBeInTheDocument()
  })

  it("uses all four transaction summary states and an exact single balance condition", async () => {
    setResponse([
      makeContact({ id: "recommended", name: "Recommended" }),
      makeContact({ id: "advanced", name: "Advanced", notify_cpfp: true }),
      makeContact({ id: "none", name: "None", notify_sending: false, notify_receiving: false, notify_sent: false, notify_received: false }),
      makeContact({ id: "custom", name: "Custom", notify_receiving: false }),
    ], [makeAlert({ contact_id: "recommended", alert_type: "below", threshold_sats: 50000000 })])

    await renderLoaded()

    expect(screen.getByText("Activity and first confirmation")).toBeInTheDocument()
    expect(screen.getByText("Activity, confirmation, and advanced alerts")).toBeInTheDocument()
    expect(screen.getByText("No transaction alerts")).toBeInTheDocument()
    expect(screen.getByText("Custom transaction alerts")).toBeInTheDocument()
    expect(screen.getByText("Balance below 0.5 BTC")).toBeInTheDocument()
  })

  it("keeps cloud admin and demo summaries read only", async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true, isLoading: false, isCloudMode: true, isSelfHostedMode: false,
      user: { id: 1, email: "admin@example.com", is_admin: true }, billingStatus: null,
    })
    await renderLoaded()
    expect(screen.queryByRole("button", { name: "Add contact" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Edit contact" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Send test" })).not.toBeInTheDocument()
  })

  it("opens the upgrade modal before a cloud user exceeds the contact limit", async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true, isLoading: false, isCloudMode: true, isSelfHostedMode: false,
      user: { id: 1, subscription_tier: "personal" },
      billingStatus: {
        subscription_tier: "personal", subscription_status: "active", stripe_customer_id: "cus_1",
        limits: { max_wallets: 1, max_contacts_per_wallet: 1, sync_interval_seconds: 60 },
        wallet_count: 1, contact_count: 1,
      },
    })
    await renderLoaded()
    await userEvent.click(screen.getByRole("button", { name: "Add contact" }))
    expect(screen.getByTestId("plans-modal")).toBeInTheDocument()
    expect(screen.queryByText("New notification destination")).not.toBeInTheDocument()
  })

  it("uses the managed ntfy topic, preserves manual edits across Back, and focuses each wizard step", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))

    const heading = screen.getByRole("heading", { name: "Where should alerts go?" })
    await waitFor(() => expect(heading).toHaveFocus())
    await user.click(screen.getByRole("combobox", { name: "Delivery method" }))
    expect(screen.queryByText("Recommended")).not.toBeInTheDocument()
    await user.keyboard("{Escape}")

    const topic = screen.getByLabelText("ntfy Topic")
    await waitFor(() => expect(topic).toHaveValue("managed-canary-topic"))
    await user.type(screen.getByLabelText("Destination name"), "Alice phone")
    expect(topic).toHaveValue("managed-canary-topic")
    await user.clear(topic)
    await user.type(topic, "alice-custom-topic")

    await user.click(screen.getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(screen.getByRole("heading", { name: "When should Canary alert you?" })).toHaveFocus())
    expect(screen.getByText("Step 2 of 3: When should Canary alert you?")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Back" }))
    await waitFor(() => expect(screen.getByRole("heading", { name: "Where should alerts go?" })).toHaveFocus())
    expect(screen.getByLabelText("ntfy Topic")).toHaveValue("alice-custom-topic")
  })

  it("generates a stable private 128-bit ntfy topic when no managed topic exists", async () => {
    mockNtfyTarget.mockReturnValue({ url: "https://ntfy.sh", defaultTopic: undefined, isBrowserSafe: true })
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    const topic = screen.getByLabelText("ntfy Topic") as HTMLInputElement
    expect(topic.value).toMatch(/^canary-[0-9a-f]{32}$/)
    const original = topic.value
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    expect(topic).toHaveValue(original)
  })

  it("warns before canceling after alert changes and Back navigation", async () => {
    const user = userEvent.setup()
    jest.mocked(window.confirm).mockReturnValue(false)
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("checkbox", { name: "Activity detected" }))
    await user.click(screen.getByRole("button", { name: "Back" }))
    await user.click(screen.getByRole("button", { name: "Cancel" }))

    expect(window.confirm).toHaveBeenCalledWith("Discard your unsaved notification changes?")
    expect(screen.getByText("New notification destination")).toBeInTheDocument()
  })

  it("maps grouped alert choices and Useful privacy to existing wire fields on creation", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("checkbox", { name: "Activity detected" }))
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(screen.getByRole("radio", { name: "Useful" })).toBeChecked()
    await user.click(screen.getByRole("button", { name: "Create destination" }))

    await waitFor(() => expect(mockApi.createContact).toHaveBeenCalledTimes(1))
    expect(mockApi.createContact).toHaveBeenCalledWith(
      wallet.checksum,
      "Desk",
      [expect.objectContaining({ notification_target: "managed-canary-topic", content_fields: USEFUL })],
      expect.objectContaining({ notify_sending: false, notify_receiving: false, notify_sent: true, notify_received: true, notify_cpfp: false, notify_rbf: false })
    )
  })

  it("supports Private, Detailed, and Custom content mappings", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("button", { name: "Continue" }))

    await user.click(screen.getByRole("radio", { name: "Private" }))
    expect(screen.getByText("Wallet activity detected")).toBeInTheDocument()
    await user.click(screen.getByRole("radio", { name: "Detailed" }))
    await user.click(screen.getByRole("button", { name: "Customize individual details" }))
    await user.click(screen.getByRole("checkbox", { name: "Transaction amount" }))
    expect(screen.getByText("Custom", { selector: "span" })).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "Create destination" }))
    await waitFor(() => expect(mockApi.createContact).toHaveBeenCalled())
    expect(mockApi.createContact.mock.calls[0][2][0].content_fields).toEqual({ ...DETAILED, transaction_amount: false })
    expect(mockApi.createContact.mock.calls[0][2][0].content_fields).not.toEqual(PRIVATE)
  })

  it("validates balance drafts, creates the contact once, then creates the alerts", async () => {
    const user = userEvent.setup()
    const order: string[] = []
    mockApi.validateBalanceAlert.mockImplementation(async () => { order.push("validate") })
    mockApi.createContact.mockImplementation(async () => { order.push("contact"); return { id: "created-contact" } })
    mockApi.createBalanceAlert.mockImplementation(async () => { order.push("alert"); return makeAlert() })
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.type(screen.getByLabelText("Alert amount"), "0.5")
    await user.click(screen.getByRole("button", { name: "Add alert" }))
    await waitFor(() => expect(mockApi.validateBalanceAlert).toHaveBeenCalledTimes(1))
    expect(mockApi.createBalanceAlert).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(screen.getByText("Balance-alert example")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Create destination" }))
    await waitFor(() => expect(mockApi.createBalanceAlert).toHaveBeenCalledTimes(1))
    expect(order).toEqual(["validate", "contact", "alert"])
  })

  it("reloads a created contact after a partial balance failure without retrying contact creation", async () => {
    const user = userEvent.setup()
    mockApi.createBalanceAlert.mockRejectedValueOnce(new Error("balance failed"))
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Desk")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.type(screen.getByLabelText("Alert amount"), "0.5")
    await user.click(screen.getByRole("button", { name: "Add alert" }))
    await user.click(screen.getByRole("button", { name: "Continue" }))
    await user.click(screen.getByRole("button", { name: "Create destination" }))
    await screen.findByText(/The destination was created, but these balance alerts failed: below 0.5 BTC/)
    expect(mockApi.createContact).toHaveBeenCalledTimes(1)
    expect(mockApi.getWalletNotifications.mock.calls.length).toBeGreaterThan(1)
  })

  it("keeps editor changes local until Save changes and Cancel makes no API calls", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.click(screen.getByRole("checkbox", { name: "Activity detected" }))
    expect(mockApi.updateContact).not.toHaveBeenCalled()
    await user.click(screen.getAllByRole("button", { name: "Cancel" })[0])
    expect(window.confirm).toHaveBeenCalled()
    expect(mockApi.updateContact).not.toHaveBeenCalled()
    expect(screen.getByRole("button", { name: "Edit contact" })).toBeInTheDocument()
  })

  it("allows a sole disabled delivery method to be re-enabled", async () => {
    const user = userEvent.setup()
    const contact = makeContact()
    contact.is_active = false
    contact.notification_methods = contact.notification_methods.map((method) => ({
      ...method,
      is_enabled: false,
    }))
    setResponse([contact])
    await renderLoaded()

    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    const enabled = screen.getByRole("checkbox", { name: "ntfy" })
    expect(enabled).not.toBeDisabled()
    await user.click(enabled)
    await user.click(screen.getByRole("button", { name: "Save changes" }))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact.mock.calls[0][3]).toEqual([
      expect.objectContaining({ provider_type: "ntfy", is_enabled: true }),
    ])
  })

  it("preserves legacy RBF and CPFP defaults when omitted fields are edited and saved", async () => {
    const user = userEvent.setup()
    const legacyContact = makeContact() as Contact & {
      notify_cpfp?: boolean
      notify_rbf?: boolean
    }
    delete legacyContact.notify_cpfp
    delete legacyContact.notify_rbf
    setResponse([legacyContact as Contact])
    await renderLoaded()

    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.clear(screen.getByLabelText("Destination name"))
    await user.type(screen.getByLabelText("Destination name"), "Legacy contact")
    await user.click(screen.getByRole("button", { name: "Save changes" }))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact.mock.calls[0][4]).toEqual(
      expect.objectContaining({ notify_rbf: true, notify_cpfp: true })
    )
  })

  it("saves the contact first and defers balance deletion until Save changes", async () => {
    const user = userEvent.setup()
    const order: string[] = []
    setResponse([makeContact()], [makeAlert()])
    mockApi.updateContact.mockImplementation(async () => { order.push("contact"); return makeContact() })
    mockApi.deleteBalanceAlert.mockImplementation(async () => { order.push("delete"); return undefined })
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.click(screen.getByRole("button", { name: "Remove balance alert" }))
    expect(mockApi.deleteBalanceAlert).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "Save changes" }))
    await waitFor(() => expect(mockApi.deleteBalanceAlert).toHaveBeenCalledWith("alert-1"))
    expect(order).toEqual(["contact", "delete"])
  })

  it("deletes removed balance alerts before creating replacements", async () => {
    const user = userEvent.setup()
    const order: string[] = []
    setResponse([makeContact()], [makeAlert()])
    mockApi.updateContact.mockImplementation(async () => { order.push("contact"); return makeContact() })
    mockApi.deleteBalanceAlert.mockImplementation(async () => { order.push("delete"); return undefined })
    mockApi.createBalanceAlert.mockImplementation(async () => { order.push("create"); return makeAlert() })
    await renderLoaded()

    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.click(screen.getByRole("button", { name: "Remove balance alert" }))
    await user.type(screen.getByLabelText("Alert amount"), "0.5")
    await user.click(screen.getByRole("button", { name: "Add alert" }))
    await waitFor(() => expect(mockApi.validateBalanceAlert).toHaveBeenCalledTimes(1))
    await user.click(screen.getByRole("button", { name: "Save changes" }))

    await waitFor(() => expect(mockApi.createBalanceAlert).toHaveBeenCalledTimes(1))
    expect(order).toEqual(["contact", "delete", "create"])
  })

  it("rejects non-decimal fiat balance syntax", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.click(screen.getByRole("combobox", { name: "Alert currency" }))
    await user.click(screen.getByRole("option", { name: "NOK" }))
    await user.type(screen.getByLabelText("Alert amount"), "0x10")
    await user.click(screen.getByRole("button", { name: "Add alert" }))

    expect(await screen.findByRole("alert")).toHaveTextContent("Enter a valid fiat amount")
    expect(mockApi.validateBalanceAlert).not.toHaveBeenCalled()
  })

  it("reloads persisted state and identifies a partial editor balance failure", async () => {
    const user = userEvent.setup()
    setResponse([makeContact()], [makeAlert()])
    mockApi.deleteBalanceAlert.mockRejectedValueOnce(new Error("delete failed"))
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    await user.click(screen.getByRole("button", { name: /Balance alerts/ }))
    await user.click(screen.getByRole("button", { name: "Remove balance alert" }))
    await user.click(screen.getByRole("button", { name: "Save changes" }))

    expect(await screen.findByRole("alert")).toHaveTextContent("delete above 1 BTC")
    expect(mockApi.updateContact).toHaveBeenCalledTimes(1)
    expect(mockApi.getWalletNotifications.mock.calls.length).toBeGreaterThan(1)
  })

  it("preserves a full webhook URL in the editor while its summary remains redacted", async () => {
    const user = userEvent.setup()
    const webhook = "https://hooks.example.com/canary?token=secret"
    setResponse([makeContact({ notification_methods: [{
      id: "webhook", contact_id: "contact-1", provider_type: "webhook", notification_target: webhook,
      created_at: "2024-01-01T00:00:00Z", is_enabled: true, content_fields: USEFUL,
    }] })])
    await renderLoaded()
    expect(screen.getByText("Webhook: https://hooks.example.com")).toBeInTheDocument()
    expect(screen.queryByText(/token=secret/)).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Edit contact" }))
    expect(screen.getByLabelText("Webhook URL")).toHaveValue(webhook)
  })

  it("requires the existing delete confirmation dialog", async () => {
    const user = userEvent.setup()
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Contact actions" }))
    await user.click(screen.getByRole("menuitem", { name: "Delete contact" }))
    expect(screen.getByRole("dialog")).toBeInTheDocument()
    expect(mockApi.deleteContact).not.toHaveBeenCalled()
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }))
    await waitFor(() => expect(mockApi.deleteContact).toHaveBeenCalledWith(wallet.checksum, "contact-1"))
  })

  it("shows transient test success and subsequent failures", async () => {
    const user = userEvent.setup()
    mockApi.sendTestNtfyNotification.mockResolvedValueOnce({ success: true }).mockResolvedValueOnce({ success: false, error: "ntfy unavailable" })
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Send test" }))
    await waitFor(() => expect(mockApi.sendTestNtfyNotification).toHaveBeenCalledTimes(1))
    expect(screen.getByRole("button", { name: "Test sent" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Test sent" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("ntfy unavailable")
    expect(screen.queryByRole("button", { name: "Test sent" })).not.toBeInTheDocument()
  })

  it("uses a method menu when several enabled methods support testing", async () => {
    const user = userEvent.setup()
    setResponse([makeContact({ notification_methods: [
      makeContact().notification_methods[0],
      { id: "nostr", contact_id: "contact-1", provider_type: "nostr", notification_target: "npub1destination", created_at: "2024-01-01T00:00:00Z", is_enabled: true, content_fields: USEFUL },
    ] })])
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Send test" }))
    expect(screen.getByRole("menuitem", { name: "ntfy" })).toBeInTheDocument()
    expect(screen.getByRole("menuitem", { name: "Nostr" })).toBeInTheDocument()
  })

  it("continues showing summaries when provider discovery fails", async () => {
    mockApi.getProviders.mockRejectedValue(new Error("offline"))
    await renderLoaded()
    expect(screen.getByRole("heading", { name: "Alice" })).toBeInTheDocument()
    expect(screen.getByText("Activity and first confirmation")).toBeInTheDocument()
  })

  it("requires cloud email verification before leaving Delivery", async () => {
    const user = userEvent.setup()
    verification.isVerified = false
    mockUseAuth.mockReturnValue({
      isAuthenticated: true, isLoading: false, isCloudMode: true, isSelfHostedMode: false,
      user: { id: 1, subscription_tier: "team" }, billingStatus: null,
    })
    setResponse([])
    await renderLoaded()
    await user.click(screen.getByRole("button", { name: "Add contact" }))
    await user.type(screen.getByLabelText("Destination name"), "Alice")
    await user.type(screen.getByPlaceholderText("your@email.com"), "alice@example.com")
    await user.click(screen.getByRole("button", { name: "Continue" }))
    expect(screen.getByText("Please verify the new email address before saving the contact")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Where should alerts go?" })).toBeInTheDocument()
  })
})
