import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { NtfyServerSettings } from "../ntfy-server-settings"
import type { NtfyServerOption } from "@/lib/ntfy-servers"

describe("NtfyServerSettings", () => {
  const localNtfy: NtfyServerOption = {
    id: "umbrel-ntfy",
    name: "ntfy",
    baseUrl: "http://ntfy_app_1",
    isLocal: true,
    isCustom: false,
  }

  const publicNtfy: NtfyServerOption = {
    id: "ntfy-sh",
    name: "ntfy",
    baseUrl: "https://ntfy.sh",
    isLocal: false,
    isCustom: false,
  }

  const defaultProps = {
    ntfyServerUrl: "http://ntfy_app_1",
    onNtfyServerUrlChange: jest.fn(),
    ntfyServers: [publicNtfy, localNtfy],
    selectedNtfyServerId: "umbrel-ntfy",
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

  it("renders an editable URL option and local Umbrel when detected", () => {
    render(<NtfyServerSettings {...defaultProps} />)

    expect(screen.getByText("Push Notifications")).toBeInTheDocument()
    expect(screen.getAllByText("ntfy")).toHaveLength(2)
    expect(screen.getByText("https://ntfy.sh")).toBeInTheDocument()
    expect(screen.getByText("Umbrel")).toBeInTheDocument()
    expect(screen.queryByText("http://ntfy_app_1")).not.toBeInTheDocument()
    expect(screen.getByText(/Create an access token in ntfy/)).toBeInTheDocument()
  })

  it("calls the server change handler when choosing an option", async () => {
    const user = userEvent.setup()
    const onNtfyServerChange = jest.fn()

    render(<NtfyServerSettings {...defaultProps} onNtfyServerChange={onNtfyServerChange} />)

    await user.click(screen.getAllByRole("radio", { name: "ntfy" })[0])

    expect(onNtfyServerChange).toHaveBeenCalledWith("ntfy-sh")
  })

  it("does not show a radio button when only public ntfy is available", () => {
    render(
      <NtfyServerSettings
        {...defaultProps}
        ntfyServers={[publicNtfy]}
        selectedNtfyServerId="ntfy-sh"
        ntfyServerUrl="https://ntfy.sh"
      />
    )

    expect(screen.getByText("ntfy")).toBeInTheDocument()
    expect(screen.queryByRole("radio")).not.toBeInTheDocument()
  })

  it("edits the public/custom URL inline", async () => {
    const user = userEvent.setup()
    const onNtfyServerUrlChange = jest.fn()
    const onNtfySettingsSave = jest.fn()
    render(
      <NtfyServerSettings
        {...defaultProps}
        selectedNtfyServerId="ntfy-sh"
        ntfyServerUrl="https://ntfy.example.com"
        onNtfyServerUrlChange={onNtfyServerUrlChange}
        onNtfySettingsSave={onNtfySettingsSave}
      />
    )

    expect(screen.getByText("https://ntfy.example.com")).toBeInTheDocument()
    expect(screen.queryByLabelText("ntfy Server URL")).not.toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /edit/i }))

    expect(screen.getByLabelText("ntfy Server URL")).toHaveValue("https://ntfy.example.com")
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument()
    await user.click(screen.getAllByRole("button", { name: /^save$/i })[0])

    expect(onNtfySettingsSave).toHaveBeenCalledTimes(1)
  })

  it("restores the public/custom URL when cancelling inline edit", async () => {
    const user = userEvent.setup()
    const onNtfyServerUrlChange = jest.fn()
    render(
      <NtfyServerSettings
        {...defaultProps}
        selectedNtfyServerId="ntfy-sh"
        ntfyServerUrl="https://ntfy.example.com"
        onNtfyServerUrlChange={onNtfyServerUrlChange}
      />
    )

    await user.click(screen.getByRole("button", { name: /edit/i }))
    await user.click(screen.getByRole("button", { name: /cancel/i }))

    expect(onNtfyServerUrlChange).toHaveBeenCalledWith("https://ntfy.example.com")
  })

  it("shows auth controls for public ntfy", () => {
    render(
      <NtfyServerSettings
        {...defaultProps}
        selectedNtfyServerId="ntfy-sh"
        ntfyServerUrl="https://ntfy.sh"
      />
    )

    expect(screen.getByText("Authentication")).toBeInTheDocument()
  })

  it("disables test notifications while ntfy settings are unsaved", () => {
    render(<NtfyServerSettings {...defaultProps} hasAnyNtfyChanges={true} />)

    expect(screen.getByRole("button", { name: "Send Test" })).toBeDisabled()
  })
})
