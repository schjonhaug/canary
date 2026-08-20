import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { TxExplorerSettings } from "../tx-explorer-settings"
import { DEFAULT_TX_EXPLORER, PUBLIC_TX_EXPLORERS, type TxExplorerOption } from "@/lib/tx-explorers"

describe("TxExplorerSettings", () => {
  const localMempool: TxExplorerOption = {
    id: "mempool",
    name: "Mempool",
    baseUrl: "http://umbrel.local:3006",
    platform: "umbrel",
    isLocal: true,
  }
  const defaultProps = {
    customExplorerUrl: "",
    savedCustomExplorerUrl: "",
    savedExplorerId: "mempool-space",
    settingsError: null,
    isUpdating: false,
    onExplorerChange: jest.fn(),
    onCustomExplorerUrlChange: jest.fn(),
    onCustomExplorerSave: jest.fn(),
  }

  it("renders public and local explorer options when local mempool is available", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={[DEFAULT_TX_EXPLORER, localMempool]}
        selectedExplorerId="mempool-space"
      />
    )

    expect(screen.getByText("Transaction Explorer")).toBeInTheDocument()
    expect(screen.queryByText("Public")).not.toBeInTheDocument()
    expect(screen.getByText("Mempool")).toBeInTheDocument()
    expect(screen.getByLabelText("https://mempool.space")).toBeInTheDocument()
    expect(screen.getByLabelText("Umbrel")).toBeInTheDocument()
    expect(screen.getByAltText("Mempool logo")).toBeInTheDocument()
    expect(screen.getByText("https://mempool.space")).toBeInTheDocument()
    expect(screen.getByText("Umbrel")).toBeInTheDocument()
    expect(screen.queryByText("http://umbrel.local:3006")).not.toBeInTheDocument()
  })

  it("renders all public explorer options", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="mempool-space"
      />
    )

    expect(screen.queryByText("Public")).not.toBeInTheDocument()
    expect(screen.queryByText("Local")).not.toBeInTheDocument()
    expect(screen.getByText("Mempool")).toBeInTheDocument()
    expect(screen.getByText("Bitfeed")).toBeInTheDocument()
    expect(screen.getByText("BTC RPC Explorer")).toBeInTheDocument()
    expect(screen.getByText("https://mempool.space")).toBeInTheDocument()
    expect(screen.getByText("https://bitfeed.live")).toBeInTheDocument()
    expect(screen.getByText("https://bitcoinexplorer.org")).toBeInTheDocument()
    expect(screen.getByLabelText("https://bitfeed.live")).toBeInTheDocument()
    expect(screen.getByLabelText("https://bitcoinexplorer.org")).toBeInTheDocument()
    expect(screen.getByLabelText("Custom URL")).toBeInTheDocument()
    expect(screen.getByLabelText("Custom explorer URL")).toBeInTheDocument()
  })

  it("falls back to a local label when a local explorer has no platform", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={[DEFAULT_TX_EXPLORER, { ...localMempool, platform: undefined }]}
        selectedExplorerId="mempool-space"
      />
    )

    expect(screen.getByText("Local")).toBeInTheDocument()
    expect(screen.queryByText("http://umbrel.local:3006")).not.toBeInTheDocument()
  })

  it("falls back to a local label when a local explorer has an unknown platform", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={[DEFAULT_TX_EXPLORER, { ...localMempool, platform: "unknown-node" }]}
        selectedExplorerId="mempool-space"
      />
    )

    expect(screen.getByText("Local")).toBeInTheDocument()
    expect(screen.queryByText("unknown-node")).not.toBeInTheDocument()
  })

  it("calls the preference update handler when choosing an option", async () => {
    const user = userEvent.setup()
    const onExplorerChange = jest.fn()

    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={[DEFAULT_TX_EXPLORER, localMempool]}
        selectedExplorerId="mempool-space"
        onExplorerChange={onExplorerChange}
      />
    )

    await user.click(screen.getByLabelText("Umbrel"))

    expect(onExplorerChange).toHaveBeenCalledWith("mempool")
  })

  it("saves a custom explorer URL template", async () => {
    const user = userEvent.setup()
    const onCustomExplorerUrlChange = jest.fn()
    const onCustomExplorerSave = jest.fn()

    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="custom"
        customExplorerUrl="https://example.com/tx/{txid}"
        savedCustomExplorerUrl=""
        onCustomExplorerUrlChange={onCustomExplorerUrlChange}
        onCustomExplorerSave={onCustomExplorerSave}
      />
    )

    expect(screen.getByText("Preview: https://example.com/tx/<transaction-id>")).toBeInTheDocument()
    await user.type(screen.getByLabelText("Custom explorer URL"), "a")
    await user.click(screen.getByRole("button", { name: /^save$/i }))

    expect(onCustomExplorerUrlChange).toHaveBeenCalled()
    expect(onCustomExplorerSave).toHaveBeenCalled()
  })

  it("disables custom explorer save until the URL template is valid", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="custom"
        customExplorerUrl=""
        savedCustomExplorerUrl=""
      />
    )

    expect(screen.getByRole("button", { name: /^save$/i })).toBeDisabled()
  })

  it("marks the custom explorer URL input invalid when an error is shown", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="custom"
        customExplorerUrl="not-a-url"
        settingsError="Invalid custom explorer URL"
      />
    )

    expect(screen.getByLabelText("Custom explorer URL")).toHaveAttribute("aria-invalid", "true")
  })

  it("selects the custom explorer option when the custom URL input is focused", async () => {
    const user = userEvent.setup()
    const onExplorerChange = jest.fn()

    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="mempool-space"
        onExplorerChange={onExplorerChange}
      />
    )

    await user.click(screen.getByLabelText("Custom explorer URL"))

    expect(onExplorerChange).toHaveBeenCalledWith("custom")
  })

  it("keeps custom explorer save enabled when switching back to a saved template that is not the active preference", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="custom"
        savedExplorerId="mempool-space"
        customExplorerUrl="https://example.com/tx/{txid}"
        savedCustomExplorerUrl="https://example.com/tx/{txid}"
      />
    )

    expect(screen.getByRole("button", { name: /^save$/i })).toBeEnabled()
  })

  it("disables custom explorer save when the custom template is already the active preference", () => {
    render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="custom"
        savedExplorerId="custom"
        customExplorerUrl="https://example.com/tx/{txid}"
        savedCustomExplorerUrl="https://example.com/tx/{txid}"
      />
    )

    expect(screen.getByRole("button", { name: /^save$/i })).toBeDisabled()
  })

  it("still renders custom explorer when only mempool.space is available", () => {
    const { container } = render(
      <TxExplorerSettings
        {...defaultProps}
        explorers={[DEFAULT_TX_EXPLORER]}
        selectedExplorerId="mempool-space"
      />
    )

    expect(container).not.toBeEmptyDOMElement()
  })
})
