import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { NtfyServerSettings } from "../ntfy-server-settings"
import type { NtfyServerOption } from "@/lib/ntfy-servers"

describe("NtfyServerSettings", () => {
  const localNtfy: NtfyServerOption = {
    id: "umbrel-ntfy",
    name: "ntfy",
    baseUrl: "http://ntfy_app_1",
    isLocal: true,
    platform: "umbrel",
    managedAuth: false,
  }

  const publicNtfy: NtfyServerOption = {
    id: "ntfy-sh",
    name: "ntfy",
    baseUrl: "https://ntfy.sh",
    isLocal: false,
    managedAuth: false,
  }

  const startosNtfy: NtfyServerOption = {
    id: "startos-ntfy",
    name: "ntfy",
    baseUrl: "http://localhost:2586",
    isLocal: true,
    platform: "startos",
    defaultTopic: "canary",
    managedAuth: true,
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
    expect(screen.getByText("ntfy")).toBeInTheDocument()
    const ntfyLogo = screen.getByAltText("ntfy logo")
    expect(ntfyLogo).toHaveClass("dark:invert")
    expect(ntfyLogo).not.toHaveClass("invert")
    expect(screen.getByText("https://ntfy.sh")).toBeInTheDocument()
    expect(screen.getByText("Umbrel")).toBeInTheDocument()
    expect(screen.getByText("Custom URL")).toBeInTheDocument()
    expect(screen.queryByText("http://ntfy_app_1")).not.toBeInTheDocument()
    expect(screen.getByText(/Create an access token in ntfy/)).toBeInTheDocument()
  })

  it("calls the server change handler when choosing an option", async () => {
    const user = userEvent.setup()
    const onNtfyServerChange = jest.fn()

    render(<NtfyServerSettings {...defaultProps} onNtfyServerChange={onNtfyServerChange} />)

    await user.click(screen.getByRole("radio", { name: "https://ntfy.sh" }))

    expect(onNtfyServerChange).toHaveBeenCalledWith("ntfy-sh")
  })

  it("shows public and custom endpoints when only public ntfy is available", () => {
    render(
      <NtfyServerSettings
        {...defaultProps}
        ntfyServers={[publicNtfy]}
        selectedNtfyServerId="ntfy-sh"
        ntfyServerUrl="https://ntfy.sh"
      />
    )

    expect(screen.getByText("ntfy")).toBeInTheDocument()
    expect(screen.getByRole("radio", { name: "https://ntfy.sh" })).toBeInTheDocument()
    expect(screen.getByRole("radio", { name: "Custom URL" })).toBeInTheDocument()
  })

  it("keeps a selected custom endpoint stable when no public ntfy entry is available", async () => {
    const user = userEvent.setup()

    function ControlledSettings() {
      const [serverUrl, setServerUrl] = useState("http://ntfy_app_1")
      const [selectedServerId, setSelectedServerId] = useState("umbrel-ntfy")

      return (
        <NtfyServerSettings
          {...defaultProps}
          ntfyServers={[localNtfy]}
          selectedNtfyServerId={selectedServerId}
          ntfyServerUrl={serverUrl}
          onNtfyServerUrlChange={setServerUrl}
          onNtfyServerChange={setSelectedServerId}
        />
      )
    }

    render(<ControlledSettings />)

    await user.click(screen.getByRole("radio", { name: "Custom URL" }))
    expect(screen.getByRole("radio", { name: "Custom URL" })).toBeChecked()
    expect(screen.getByLabelText("ntfy Server URL")).toHaveValue("")
  })

  it("edits the custom URL inline", async () => {
    const user = userEvent.setup()
    const onNtfyServerChange = jest.fn()

    function ControlledSettings() {
      const [serverUrl, setServerUrl] = useState("https://ntfy.example.com")

      return (
        <NtfyServerSettings
          {...defaultProps}
          selectedNtfyServerId="ntfy-sh"
          ntfyServerUrl={serverUrl}
          onNtfyServerUrlChange={setServerUrl}
          onNtfyServerChange={onNtfyServerChange}
        />
      )
    }

    render(<ControlledSettings />)

    expect(screen.getByLabelText("ntfy Server URL")).toHaveValue("https://ntfy.example.com")

    await user.clear(screen.getByLabelText("ntfy Server URL"))
    await user.type(screen.getByLabelText("ntfy Server URL"), "https://ntfy.changed.example.com")

    expect(screen.getByLabelText("ntfy Server URL")).toHaveValue("https://ntfy.changed.example.com")
  })

  it("keeps the custom URL editor visible when validation fails", async () => {
    const user = userEvent.setup()

    function ControlledSettings() {
      const [serverUrl, setServerUrl] = useState("https://ntfy.example.com")

      return (
        <NtfyServerSettings
          {...defaultProps}
          selectedNtfyServerId="ntfy-sh"
          ntfyServerUrl={serverUrl}
          onNtfyServerUrlChange={setServerUrl}
          ntfySettingsError="URL must start with http:// or https://"
        />
      )
    }

    render(<ControlledSettings />)

    await user.clear(screen.getByLabelText("ntfy Server URL"))
    await user.type(screen.getByLabelText("ntfy Server URL"), "ntfy.example.com")

    expect(screen.getByLabelText("ntfy Server URL")).toBeInTheDocument()
    expect(screen.getByText("URL must start with http:// or https://")).toBeInTheDocument()
  })

  it("switches between public, local, and custom endpoints", async () => {
    const user = userEvent.setup()

    function ControlledSettings() {
      const [serverUrl, setServerUrl] = useState("https://ntfy.example.com")
      const [selectedServerId, setSelectedServerId] = useState("ntfy-sh")

      return (
        <NtfyServerSettings
          {...defaultProps}
          selectedNtfyServerId={selectedServerId}
          ntfyServerUrl={serverUrl}
          onNtfyServerUrlChange={setServerUrl}
          onNtfyServerChange={(serverId) => {
            setSelectedServerId(serverId)
            setServerUrl(serverId === "umbrel-ntfy" ? "http://ntfy_app_1" : "https://ntfy.sh")
          }}
        />
      )
    }

    render(<ControlledSettings />)

    await user.click(screen.getByRole("radio", { name: "Umbrel" }))
    expect(screen.queryByLabelText("ntfy Server URL")).not.toBeInTheDocument()

    await user.click(screen.getByRole("radio", { name: "Custom URL" }))
    expect(screen.getByLabelText("ntfy Server URL")).toHaveValue("")
  })

  it("closes the public URL editor when switching servers", async () => {
    const user = userEvent.setup()

    function ControlledSettings() {
      const [selectedServerId, setSelectedServerId] = useState("ntfy-sh")
      const [serverUrl, setServerUrl] = useState("https://ntfy.example.com")

      return (
        <NtfyServerSettings
          {...defaultProps}
          selectedNtfyServerId={selectedServerId}
          ntfyServerUrl={serverUrl}
          onNtfyServerUrlChange={setServerUrl}
          onNtfyServerChange={(serverId) => {
            setSelectedServerId(serverId)
            setServerUrl(serverId === "umbrel-ntfy" ? "http://ntfy_app_1" : "https://ntfy.example.com")
          }}
        />
      )
    }

    render(<ControlledSettings />)

    expect(screen.getByLabelText("ntfy Server URL")).toBeInTheDocument()

    await user.click(screen.getByRole("radio", { name: "Umbrel" }))
    expect(screen.queryByLabelText("ntfy Server URL")).not.toBeInTheDocument()

    await user.click(screen.getByRole("radio", { name: "Custom URL" }))
    expect(screen.getByLabelText("ntfy Server URL")).toBeInTheDocument()
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

  it("defaults test notifications to the managed server topic without clobbering edits", async () => {
    const user = userEvent.setup()
    const { rerender } = render(
      <NtfyServerSettings
        {...defaultProps}
        ntfyServers={[publicNtfy, startosNtfy]}
        selectedNtfyServerId="startos-ntfy"
        ntfyServerUrl="http://localhost:2586"
      />
    )

    const topicInput = document.querySelector("#ntfy-test-topic")
    expect(topicInput).toHaveValue("canary")

    await user.clear(topicInput as HTMLInputElement)
    await user.type(topicInput as HTMLInputElement, "custom-topic")

    rerender(
      <NtfyServerSettings
        {...defaultProps}
        ntfyServers={[publicNtfy, { ...startosNtfy, defaultTopic: "changed-topic" }]}
        selectedNtfyServerId="startos-ntfy"
        ntfyServerUrl="http://localhost:2586"
      />
    )

    expect(document.querySelector("#ntfy-test-topic")).toHaveValue("custom-topic")
  })
})
