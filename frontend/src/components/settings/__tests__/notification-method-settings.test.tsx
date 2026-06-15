import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { NotificationMethodSettings } from "../notification-method-settings"
import type { NtfyServerOption } from "@/lib/ntfy-servers"

jest.mock("@/lib/api", () => {
  const actual = jest.requireActual("@/lib/api")
  return {
    ApiError: actual.ApiError,
    api: {
      getNostrSettings: jest.fn(),
      sendTestNostrNotification: jest.fn(),
      sendTestNtfyNotification: jest.fn(),
    },
  }
})

const mockApi = jest.requireMock("@/lib/api").api

describe("NotificationMethodSettings", () => {
  const publicNtfy: NtfyServerOption = {
    id: "ntfy-sh",
    name: "ntfy",
    baseUrl: "https://ntfy.sh",
    isLocal: false,
    managedAuth: false,
  }

  const defaultProps = {
    ntfyServerUrl: "https://ntfy.sh",
    onNtfyServerUrlChange: jest.fn(),
    ntfyServers: [publicNtfy],
    selectedNtfyServerId: "ntfy-sh",
    onNtfyServerChange: jest.fn(),
    userPreferences: {
      preferred_fiat_currency: "USD",
      preferred_tx_explorer_id: null,
      ntfy_server_url: null,
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    },
    ntfyAuthType: "none" as const,
    onNtfyAuthTypeChange: jest.fn(),
    ntfyAccessToken: "",
    onNtfyAccessTokenChange: jest.fn(),
    ntfyUsername: "",
    onNtfyUsernameChange: jest.fn(),
    ntfyPassword: "",
    onNtfyPasswordChange: jest.fn(),
    hasAnyNtfyChanges: false,
    isUpdatingNtfySettings: false,
    ntfySettingsError: null,
    ntfySettingsSuccess: false,
    onNtfySettingsSave: jest.fn(),
    onClearNtfySettingsErrors: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getNostrSettings.mockResolvedValue({
      sender_npub: "npub1canarysender",
    })
  })

  it("renders ntfy and Nostr as collapsed notification method panels", () => {
    render(<NotificationMethodSettings {...defaultProps} />)

    expect(screen.getByText("Notification methods")).toBeInTheDocument()
    expect(screen.getByText("Push Notifications")).toBeInTheDocument()
    expect(screen.getByText("Nostr DMs")).toBeInTheDocument()
    expect(screen.queryByRole("radio", { name: "https://ntfy.sh" })).not.toBeInTheDocument()
    expect(screen.queryByLabelText("Canary sender npub")).not.toBeInTheDocument()
  })

  it("expands one notification method at a time", async () => {
    const user = userEvent.setup()
    render(<NotificationMethodSettings {...defaultProps} />)

    await user.click(screen.getByRole("button", { name: /Push Notifications/ }))
    expect(screen.getByRole("radio", { name: "https://ntfy.sh" })).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /Nostr DMs/ }))

    expect(screen.queryByRole("radio", { name: "https://ntfy.sh" })).not.toBeInTheDocument()
    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })
  })
})
