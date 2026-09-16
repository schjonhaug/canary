import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { TelegramSettingsContent } from "../telegram-settings"

jest.mock("@/lib/api", () => {
  const actual = jest.requireActual("@/lib/api")
  return {
    ApiError: actual.ApiError,
    api: {
      getTelegramSettings: jest.fn(),
      updateTelegramSettings: jest.fn(),
      sendTestTelegramNotification: jest.fn(),
    },
  }
})

const mockApi = jest.requireMock("@/lib/api").api

async function readyTokenField() {
  const tokenField = await screen.findByLabelText("Bot token")
  await waitFor(() => expect(tokenField).toBeEnabled())
  return tokenField
}

describe("TelegramSettingsContent", () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getTelegramSettings.mockResolvedValue({ configured: false })
    mockApi.updateTelegramSettings.mockImplementation(async (botToken: string) => {
      const configured = botToken.trim().length > 0
      mockApi.getTelegramSettings.mockResolvedValue({ configured })
      return { configured }
    })
    mockApi.sendTestTelegramNotification.mockResolvedValue({ success: true })
  })

  it("saves a bot token without echoing it back", async () => {
    const user = userEvent.setup()
    render(<TelegramSettingsContent />)

    const tokenField = await readyTokenField()
    expect(tokenField).toHaveAttribute("type", "password")
    expect(tokenField).toHaveAttribute("placeholder", "123456:ABC-DEF")

    await user.type(tokenField, "123456:ABC-DEF")
    await user.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() => expect(mockApi.updateTelegramSettings).toHaveBeenCalledWith("123456:ABC-DEF"))
    expect(await screen.findByText("Saved successfully!")).toBeInTheDocument()
    expect(tokenField).toHaveValue("")
    expect(tokenField).toHaveAttribute("placeholder", "••••••••")
    expect(screen.queryByDisplayValue("123456:ABC-DEF")).not.toBeInTheDocument()
  })

  it("disables Send Test while a new token is unsaved", async () => {
    mockApi.getTelegramSettings.mockResolvedValue({ configured: true })
    const user = userEvent.setup()
    render(<TelegramSettingsContent />)

    const tokenField = await readyTokenField()
    await user.type(screen.getByLabelText("Test chat ID"), "123456789")
    expect(screen.getByRole("button", { name: "Send Test" })).toBeEnabled()

    await user.type(tokenField, "new-token")
    expect(screen.getByRole("button", { name: "Send Test" })).toBeDisabled()
  })

  it("sends a test after a token is saved", async () => {
    const user = userEvent.setup()
    render(<TelegramSettingsContent />)

    await user.type(await readyTokenField(), "123456:ABC-DEF")
    await user.click(screen.getByRole("button", { name: "Save" }))
    await screen.findByText("Saved successfully!")

    await user.type(screen.getByLabelText("Test chat ID"), "123456789")
    await user.click(screen.getByRole("button", { name: "Send Test" }))

    await waitFor(() => expect(mockApi.sendTestTelegramNotification).toHaveBeenCalledWith("123456789"))
    expect(screen.getByText("Test Telegram message sent successfully!")).toBeInTheDocument()
  })
})
