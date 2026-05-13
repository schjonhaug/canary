import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useUserPreferences } from "../useUserPreferences"

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

function PreferencesProbe() {
  const preferences = useUserPreferences({ isAuthenticated: true })

  return (
    <div>
      <div data-testid="selected-ntfy-server">{preferences.selectedNtfyServerId}</div>
      <div data-testid="has-ntfy-changes">{String(preferences.hasAnyNtfyChanges)}</div>
      <button type="button" onClick={() => preferences.handleNtfyServerChange("ntfy-sh")}>
        Select public
      </button>
      <button type="button" onClick={() => preferences.handleNtfyServerChange("umbrel-ntfy")}>
        Select local
      </button>
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
        { id: "umbrel-ntfy", name: "ntfy", base_url: "http://ntfy_app_1", platform: "umbrel" },
      ],
      default_ntfy_server_id: "umbrel-ntfy",
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
})
