import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { BalanceDraftControls } from "../balance-draft-controls"

Object.defineProperties(Element.prototype, {
  hasPointerCapture: { value: jest.fn(() => false) },
  setPointerCapture: { value: jest.fn() },
  releasePointerCapture: { value: jest.fn() },
  scrollIntoView: { value: jest.fn() },
})

jest.mock("@/lib/api", () => ({
  api: {
    validateBalanceAlert: jest.fn(),
  },
  ApiError: class ApiError extends Error {},
}))

describe("BalanceDraftControls", () => {
  const readyBalance = { status: "ready" as const, sats: 50_000_000, fiat: 25000, fiatCurrency: "USD" }

  it("shows the current alert balance when expanded and fills BTC without rounding sats", async () => {
    const user = userEvent.setup()
    render(
      <BalanceDraftControls
        walletChecksum="sq32h3ch"
        value={[]}
        onChange={jest.fn()}
        preferredFiatCurrency="USD"
        alertBalance={readyBalance}
        defaultOpen
      />
    )

    expect(screen.getByText(/Current balance/)).toBeInTheDocument()
    expect(screen.getByText(/0\.5 BTC/)).toBeInTheDocument()
    expect(screen.getByText(/50,000,000 sats/)).toBeInTheDocument()
    expect(screen.getByText(/including unconfirmed funds/)).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "Use current balance" }))
    expect(screen.getByLabelText("Alert amount")).toHaveValue("0.5")
  })

  it("fills sats when that unit is selected and does not change the condition", async () => {
    const user = userEvent.setup()
    render(
      <BalanceDraftControls
        walletChecksum="sq32h3ch"
        value={[]}
        onChange={jest.fn()}
        preferredFiatCurrency="USD"
        alertBalance={readyBalance}
        defaultOpen
      />
    )

    expect(screen.getByRole("radio", { name: /Below/ })).toBeChecked()
    await user.click(screen.getByRole("combobox", { name: "Alert currency" }))
    await user.click(screen.getByRole("option", { name: "sats" }))
    await user.click(screen.getByRole("button", { name: "Use current balance" }))

    expect(screen.getByLabelText("Alert amount")).toHaveValue("50000000")
    expect(screen.getByRole("radio", { name: /Below/ })).toBeChecked()
  })

  it("does not overwrite a typed amount when the displayed balance stays the same", async () => {
    const user = userEvent.setup()
    const { rerender } = render(
      <BalanceDraftControls
        walletChecksum="sq32h3ch"
        value={[]}
        onChange={jest.fn()}
        preferredFiatCurrency="USD"
        alertBalance={readyBalance}
        defaultOpen
      />
    )

    await user.type(screen.getByLabelText("Alert amount"), "0.21")
    rerender(
      <BalanceDraftControls
        walletChecksum="sq32h3ch"
        value={[]}
        onChange={jest.fn()}
        preferredFiatCurrency="USD"
        alertBalance={{ ...readyBalance, sats: 51_000_000 }}
        defaultOpen
      />
    )

    expect(screen.getByLabelText("Alert amount")).toHaveValue("0.21")
    expect(screen.getByText(/0\.51 BTC/)).toBeInTheDocument()
  })

  it("does not treat an unknown balance as zero", () => {
    render(
      <BalanceDraftControls
        walletChecksum="sq32h3ch"
        value={[]}
        onChange={jest.fn()}
        preferredFiatCurrency="USD"
        alertBalance={{ status: "unavailable" }}
        defaultOpen
      />
    )

    expect(screen.getByText(/Current balance is unavailable/)).toBeInTheDocument()
    expect(screen.queryByText(/0 BTC/)).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Use current balance" })).toBeDisabled()
  })
})
