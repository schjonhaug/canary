import {
  DEFAULT_TX_EXPLORER,
  buildTransactionExplorerUrl,
  buildTxExplorerOptions,
  resolveSelectedTxExplorer,
} from "../tx-explorers"

describe("tx explorer helpers", () => {
  const location = {
    protocol: "http:",
    hostname: "umbrel.local",
  }

  it("builds explorer options from explicit base URLs and local ports", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [
          { id: "mempool", name: "Mempool", base_url: "http://umbrel.local:3006/", port: null },
          { id: "bitfeed", name: "Bitfeed", base_url: null, port: 8314 },
        ],
        default_tx_explorer_id: "mempool-space",
      },
      location
    )

    expect(options).toEqual([
      DEFAULT_TX_EXPLORER,
      { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
      { id: "bitfeed", name: "Bitfeed", baseUrl: "http://umbrel.local:8314", isLocal: true },
    ])
  })

  it("prefers a saved explorer selection when available", () => {
    const selected = resolveSelectedTxExplorer(
      [
        DEFAULT_TX_EXPLORER,
        { id: "bitfeed", name: "Bitfeed", baseUrl: "http://umbrel.local:8314", isLocal: true },
      ],
      "bitfeed",
      "mempool-space"
    )

    expect(selected.id).toBe("bitfeed")
  })

  it("falls back to the single local explorer when no preference exists", () => {
    const selected = resolveSelectedTxExplorer(
      [
        DEFAULT_TX_EXPLORER,
        { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
      ],
      null,
      "mempool-space"
    )

    expect(selected.id).toBe("mempool")
  })

  it("builds transaction URLs with a shared /tx path", () => {
    expect(buildTransactionExplorerUrl("http://umbrel.local:3006/", "abc123")).toBe(
      "http://umbrel.local:3006/tx/abc123"
    )
  })
})
