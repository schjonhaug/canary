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

  it("renders public and local explorer options when local mempool is available", () => {
    render(
      <TxExplorerSettings
        explorers={[DEFAULT_TX_EXPLORER, localMempool]}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={jest.fn()}
      />
    )

    expect(screen.getByText("Transaction Explorer")).toBeInTheDocument()
    expect(screen.getByText("Public")).toBeInTheDocument()
    expect(screen.getByText("Local")).toBeInTheDocument()
    expect(screen.getByLabelText("Mempool Space")).toBeInTheDocument()
    expect(screen.getByLabelText("Mempool")).toBeInTheDocument()
    expect(screen.getByAltText("Mempool Space logo")).toBeInTheDocument()
    expect(screen.getByAltText("Mempool logo")).toBeInTheDocument()
    expect(screen.getByText("https://mempool.space")).toBeInTheDocument()
    expect(screen.getByText("Umbrel")).toBeInTheDocument()
    expect(screen.queryByText("http://umbrel.local:3006")).not.toBeInTheDocument()
  })

  it("renders all public explorer options", () => {
    render(
      <TxExplorerSettings
        explorers={PUBLIC_TX_EXPLORERS}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={jest.fn()}
      />
    )

    expect(screen.getByText("Public")).toBeInTheDocument()
    expect(screen.queryByText("Local")).not.toBeInTheDocument()
    expect(screen.getByLabelText("Mempool Space")).toBeInTheDocument()
    expect(screen.getByLabelText("Bitfeed")).toBeInTheDocument()
    expect(screen.getByLabelText("BTC RPC Explorer")).toBeInTheDocument()
    expect(screen.getByText("https://mempool.space")).toBeInTheDocument()
    expect(screen.getByText("https://bitfeed.live")).toBeInTheDocument()
    expect(screen.getByText("https://bitcoinexplorer.org")).toBeInTheDocument()
  })

  it("falls back to a local label when a local explorer has no platform", () => {
    render(
      <TxExplorerSettings
        explorers={[DEFAULT_TX_EXPLORER, { ...localMempool, platform: undefined }]}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={jest.fn()}
      />
    )

    expect(screen.getAllByText("Local")).toHaveLength(2)
    expect(screen.queryByText("http://umbrel.local:3006")).not.toBeInTheDocument()
  })

  it("falls back to a local label when a local explorer has an unknown platform", () => {
    render(
      <TxExplorerSettings
        explorers={[DEFAULT_TX_EXPLORER, { ...localMempool, platform: "unknown-node" }]}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={jest.fn()}
      />
    )

    expect(screen.getAllByText("Local")).toHaveLength(2)
    expect(screen.queryByText("unknown-node")).not.toBeInTheDocument()
  })

  it("calls the preference update handler when choosing an option", async () => {
    const user = userEvent.setup()
    const onExplorerChange = jest.fn()

    render(
      <TxExplorerSettings
        explorers={[DEFAULT_TX_EXPLORER, localMempool]}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={onExplorerChange}
      />
    )

    await user.click(screen.getByLabelText("Mempool"))

    expect(onExplorerChange).toHaveBeenCalledWith("mempool")
  })

  it("does not render when only mempool.space is available", () => {
    const { container } = render(
      <TxExplorerSettings
        explorers={[DEFAULT_TX_EXPLORER]}
        selectedExplorerId="mempool-space"
        isUpdating={false}
        onExplorerChange={jest.fn()}
      />
    )

    expect(container).toBeEmptyDOMElement()
  })
})
