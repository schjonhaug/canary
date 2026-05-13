import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useUserPreferences } from "../useUserPreferences"
import { PUBLIC_NTFY_SERVER_ID, UMBREL_NTFY_SERVER_ID } from "@/lib/ntfy-servers"

jest.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: jest.fn() }),
}))

jest.mock("@/lib/api", () => ({
  api: {
    getUserPreferences: jest.fn(),
    getConfig: jest.fn(),
    updateUserPreferences: jest.fn(),
  },
}))

const mockApi = jest.requireMock("@/lib/api").api

function PreferencesProbe({ isAuthenticated = true }: { isAuthenticated?: boolean }) {
  const preferences = useUserPreferences({ isAuthenticated })

  return (
    <div>
      <div data-testid="selected-ntfy-server">{preferences.selectedNtfyServerId}</div>
      <div data-testid="has-ntfy-changes">{String(preferences.hasAnyNtfyChanges)}</div>
      <input
        aria-label="ntfy server url"
        value={preferences.ntfyServerUrl}
        onChange={(event) => preferences.setNtfyServerUrl(event.target.value)}
      />
      <button
        type="button"
        onClick={() => preferences.handleNtfyServerChange(PUBLIC_NTFY_SERVER_ID)}
      >
        Select public
      </button>
      <button
        type="button"
        onClick={() => preferences.handleNtfyServerChange(UMBREL_NTFY_SERVER_ID)}
      >
        Select local
      </button>
      <button type="button" onClick={() => preferences.handleNtfySettingsSave()}>
        Save ntfy
      </button>
      <div data-testid="ntfy-settings-error">{preferences.ntfySettingsError ?? ""}</div>
    </div>
  )
}

describe("useUserPreferences", () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getUserPreferences.mockResolvedValue({
      preferred_fiat_currency: "USD",
      preferred_tx_explorer_id: null,
      ntfy_server_url: null,
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })
    mockApi.getConfig.mockResolvedValue({
      tx_explorers: [],
      default_tx_explorer_id: "mempool-space",
      ntfy_servers: [
        {
          id: UMBREL_NTFY_SERVER_ID,
          name: "ntfy",
          base_url: "http://ntfy_app_1",
          platform: "umbrel",
        },
      ],
      default_ntfy_server_id: UMBREL_NTFY_SERVER_ID,
    })
  })

  it("marks ntfy settings dirty when only the selected server changes", async () => {
    const user = userEvent.setup()
    render(<PreferencesProbe />)

    await waitFor(() => {
      expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent("umbrel-ntfy")
      expect(screen.getByTestId("has-ntfy-changes")).toHaveTextContent("false")
    })

    await user.click(screen.getByRole("button", { name: "Select public" }))

    expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent("ntfy-sh")
    expect(screen.getByTestId("has-ntfy-changes")).toHaveTextContent("true")
  })

  it("resolves a saved local ntfy URL to the detected local server", async () => {
    mockApi.getUserPreferences.mockResolvedValue({
      preferred_fiat_currency: "USD",
      preferred_tx_explorer_id: null,
      ntfy_server_url: "http://ntfy_app_1",
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })

    render(<PreferencesProbe />)

    await waitFor(() => {
      expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent("umbrel-ntfy")
      expect(screen.getByTestId("has-ntfy-changes")).toHaveTextContent("false")
    })
  })

  it("selects a detected local ntfy server in unauthenticated self-hosted mode", async () => {
    render(<PreferencesProbe isAuthenticated={false} />)

    await waitFor(() => {
      expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent(UMBREL_NTFY_SERVER_ID)
      expect(screen.getByTestId("has-ntfy-changes")).toHaveTextContent("false")
    })

    expect(mockApi.getUserPreferences).not.toHaveBeenCalled()
  })

  it("trims custom ntfy URLs before validation and storage", async () => {
    const user = userEvent.setup()
    mockApi.updateUserPreferences.mockResolvedValue({
      preferred_fiat_currency: "USD",
      preferred_tx_explorer_id: null,
      ntfy_server_url: "https://ntfy.example.com",
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })

    render(<PreferencesProbe />)

    await waitFor(() => {
      expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent(UMBREL_NTFY_SERVER_ID)
    })

    await user.click(screen.getByRole("button", { name: "Select public" }))
    await user.clear(screen.getByRole("textbox", { name: "ntfy server url" }))
    await user.type(
      screen.getByRole("textbox", { name: "ntfy server url" }),
      "  https://ntfy.example.com/  "
    )
    await user.click(screen.getByRole("button", { name: "Save ntfy" }))

    await waitFor(() => {
      expect(mockApi.updateUserPreferences).toHaveBeenCalledWith({
        ntfy_server_url: "https://ntfy.example.com",
      })
    })
    expect(screen.getByTestId("ntfy-settings-error")).toHaveTextContent("")
  })

  it("saves an empty ntfy URL when the detected local server is selected", async () => {
    const user = userEvent.setup()
    mockApi.updateUserPreferences.mockResolvedValue({
      preferred_fiat_currency: "USD",
      preferred_tx_explorer_id: null,
      ntfy_server_url: "",
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })

    render(<PreferencesProbe />)

    await waitFor(() => {
      expect(screen.getByTestId("selected-ntfy-server")).toHaveTextContent(UMBREL_NTFY_SERVER_ID)
    })

    await user.click(screen.getByRole("button", { name: "Save ntfy" }))

    await waitFor(() => {
      expect(mockApi.updateUserPreferences).toHaveBeenCalledWith({
        ntfy_server_url: "",
      })
    })
  })
})
