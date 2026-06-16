import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { NostrSettings } from "../nostr-settings"

Object.defineProperties(Element.prototype, {
  hasPointerCapture: {
    value: jest.fn(() => false),
  },
  setPointerCapture: {
    value: jest.fn(),
  },
  releasePointerCapture: {
    value: jest.fn(),
  },
  scrollIntoView: {
    value: jest.fn(),
  },
})

jest.mock("@/lib/api", () => {
  const actual = jest.requireActual("@/lib/api")
  return {
    ApiError: actual.ApiError,
    api: {
      getNostrSettings: jest.fn(),
      updateNostrSettings: jest.fn(),
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
      dm_mode: "auto",
    })
    mockApi.updateNostrSettings.mockResolvedValue({
      sender_npub: "npub1canarysender",
      dm_mode: "nip04",
    })
    mockApi.sendTestNostrNotification.mockResolvedValue({
      success: true,
      dm_mode_used: "nip04",
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
      expect(mockApi.sendTestNostrNotification).toHaveBeenCalledWith("npub1recipient", "auto")
      expect(screen.getByText("Test Nostr DM sent successfully with NIP-04.")).toBeInTheDocument()
    })
  })

  it("loads and saves the selected Nostr DM format", async () => {
    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.click(screen.getByRole("combobox", { name: "DM format" }))
    await user.click(await screen.findByRole("option", { name: "Legacy NIP-04" }))

    await waitFor(() => {
      expect(mockApi.updateNostrSettings).toHaveBeenCalledWith("nip04")
      expect(screen.getByText("Nostr DM format saved.")).toBeInTheDocument()
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
      error: "Nostr publish timed out",
      error_code: "nostr_publish_timeout",
    })

    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.type(screen.getByLabelText("Test recipient"), "npub1recipient")
    await user.click(screen.getByRole("button", { name: "Send Test" }))

    expect(
      await screen.findByText("Publishing the Nostr DM timed out. Check the recipient relay setup and try again.")
    ).toBeInTheDocument()
    expect(screen.queryByText("Nostr publish timed out")).not.toBeInTheDocument()
  })

  it.each([
    {
      errorCode: "nostr_inbox_discovery_timeout",
      rawError: "Nostr inbox relay discovery timed out",
      translated: "Could not find the recipient's Nostr DM inbox relays in time.",
    },
    {
      errorCode: "nostr_no_dm_relays",
      rawError: "Recipient has no kind 10050 Nostr DM inbox relay list",
      translated: "The recipient has not enabled modern NIP-17 Nostr DMs. Try Auto or Legacy NIP-04 mode.",
    },
    {
      errorCode: "nostr_nip04_failed",
      rawError: "Nostr legacy DM publish failed: no relay accepted the message",
      translated: "Could not send the legacy NIP-04 Nostr DM. Check relay connectivity and try again.",
    },
  ])("shows translated $errorCode test send errors", async ({ errorCode, rawError, translated }) => {
    mockApi.sendTestNostrNotification.mockResolvedValue({
      success: false,
      error: rawError,
      error_code: errorCode,
    })

    const user = userEvent.setup()
    render(<NostrSettings />)

    await waitFor(() => {
      expect(screen.getByLabelText("Canary sender npub")).toHaveValue("npub1canarysender")
    })

    await user.type(screen.getByLabelText("Test recipient"), "npub1recipient")
    await user.click(screen.getByRole("button", { name: "Send Test" }))

    expect(await screen.findByText(translated)).toBeInTheDocument()
    expect(screen.queryByText(rawError)).not.toBeInTheDocument()
  })
})
