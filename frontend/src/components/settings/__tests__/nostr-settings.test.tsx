import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { NostrSettings } from "../nostr-settings"

jest.mock("@/lib/api", () => {
  const actual = jest.requireActual("@/lib/api")
  return {
    ApiError: actual.ApiError,
    api: {
      getNostrSettings: jest.fn(),
      sendTestNostrNotification: jest.fn(),
    },
  }
})

const mockApi = jest.requireMock("@/lib/api").api

describe("NostrSettings", () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getNostrSettings.mockResolvedValue({
      sender_npub: "npub1canarysender",
    })
    mockApi.sendTestNostrNotification.mockResolvedValue({
      success: true,
      error: null,
    })
  })

  it("shows the generated sender npub", async () => {
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })
  })

  it("sends a test Nostr DM to the entered recipient", async () => {
    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.type(screen.getByLabelText("Test recipient"), "npub1recipient")
    await user.click(screen.getByRole("button", { name: "Send Test" }))

    await waitFor(() => {
      expect(mockApi.sendTestNostrNotification).toHaveBeenCalledWith("npub1recipient")
      expect(screen.getByText("Test Nostr DM sent successfully!")).toBeInTheDocument()
    })
  })

  it("requires a recipient before sending a test DM", async () => {
    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.click(screen.getByRole("button", { name: "Send Test" }))

    expect(await screen.findByText("Nostr recipient is required")).toBeInTheDocument()
    expect(mockApi.sendTestNostrNotification).not.toHaveBeenCalled()
  })

  it("shows translated test send errors from backend error codes", async () => {
    mockApi.sendTestNostrNotification.mockResolvedValue({
      success: false,
      error: "Nostr send timed out",
      error_code: "nostr_send_timeout",
    })

    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.type(screen.getByLabelText("Test recipient"), "npub1recipient")
    await user.click(screen.getByRole("button", { name: "Send Test" }))

    expect(
      await screen.findByText("Nostr send timed out. Check the recipient relay setup and try again.")
    ).toBeInTheDocument()
    expect(screen.queryByText("Nostr send timed out")).not.toBeInTheDocument()
  })
})
