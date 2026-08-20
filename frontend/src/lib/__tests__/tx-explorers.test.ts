import {
  DEFAULT_TX_EXPLORER,
  PUBLIC_TX_EXPLORERS,
  buildTransactionExplorerUrl,
  buildTxExplorerOptions,
  encodeCustomTxExplorerPreference,
  resolveExplorerBaseUrl,
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
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      location
    )

    expect(options).toEqual([
      ...PUBLIC_TX_EXPLORERS,
      { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
      { id: "bitfeed", name: "Bitfeed", baseUrl: "http://umbrel.local:8314", isLocal: true },
    ])
  })

  it("offers all public explorers by default", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      location
    )

    expect(options).toEqual(PUBLIC_TX_EXPLORERS)
    expect(DEFAULT_TX_EXPLORER.id).toBe("mempool-space")
  })

  it("chooses a local candidate URL matching the current browser hostname", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [
          {
            id: "mempool",
            name: "Mempool",
            base_url: null,
            base_urls: [
              "https://192.0.2.10:52127",
              "https://example-node.local:52127",
              "https://203.0.113.10:52127",
            ],
            port: null,
          },
        ],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      {
        protocol: "https:",
        hostname: "example-node.local",
      }
    )

    expect(options).toContainEqual({
      id: "mempool",
      name: "Mempool",
      baseUrl: "https://example-node.local:52127",
      isLocal: true,
    })
  })

  it("carries platform labels for local explorer options", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [
          {
            id: "mempool",
            name: "Mempool",
            base_url: null,
            base_urls: ["http://mynode.local:4080"],
            port: null,
            platform: "mynode",
          },
        ],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      {
        protocol: "http:",
        hostname: "mynode.local",
      }
    )

    expect(options).toContainEqual({
      id: "mempool",
      name: "Mempool",
      baseUrl: "http://mynode.local:4080",
      isLocal: true,
      platform: "mynode",
    })
  })

  it("chooses a local candidate URL matching the current browser protocol", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [
          {
            id: "mempool",
            name: "Mempool",
            base_url: "https://fallback.example:3006",
            base_urls: [
              "http://example-node.local:52127",
              "https://example-node.local:52127",
            ],
            port: null,
          },
        ],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      {
        protocol: "https:",
        hostname: "example-node.local",
      }
    )

    expect(options).toContainEqual({
      id: "mempool",
      name: "Mempool",
      baseUrl: "https://example-node.local:52127",
      isLocal: true,
    })
  })

  it("falls back to the first candidate URL when no browser hostname matches", () => {
    const options = buildTxExplorerOptions(
      {
        tx_explorers: [
          {
            id: "btc-rpc-explorer",
            name: "BTC RPC Explorer",
            base_url: null,
            base_urls: [
              "https://192.0.2.10:49389",
              "https://example-node.local:49389",
            ],
            port: null,
          },
        ],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      },
      {
        protocol: "https:",
        hostname: "example.onion",
      }
    )

    expect(options).toContainEqual({
      id: "btc-rpc-explorer",
      name: "BTC RPC Explorer",
      baseUrl: "https://192.0.2.10:49389",
      isLocal: true,
    })
  })

  it("resolves explorer base URLs before falling back to explicit base URL and port", () => {
    expect(
      resolveExplorerBaseUrl(
        {
          id: "mempool",
          name: "Mempool",
          base_url: "https://fallback.example:3006",
          base_urls: ["https://example-node.local:52127"],
          port: 3006,
        },
        { protocol: "https:", hostname: "example-node.local" }
      )
    ).toBe("https://example-node.local:52127")

    expect(
      resolveExplorerBaseUrl(
        {
          id: "mempool",
          name: "Mempool",
          base_url: "https://fallback.example:3006/",
          port: 3006,
        },
        { protocol: "https:", hostname: "example-node.local" }
      )
    ).toBe("https://fallback.example:3006")

    expect(
      resolveExplorerBaseUrl(
        {
          id: "mempool",
          name: "Mempool",
          base_url: null,
          port: 3006,
        },
        { protocol: "https:", hostname: "example-node.local" }
      )
    ).toBe("https://example-node.local:3006")
  })

  it("uses saved mempool-space preference even when local mempool is available", () => {
    const selected = resolveSelectedTxExplorer(
      [
        DEFAULT_TX_EXPLORER,
        { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
      ],
      "mempool-space",
      "mempool-space"
    )

    expect(selected.id).toBe("mempool-space")
  })

  it("uses saved mempool preference when local mempool is available", () => {
    const selected = resolveSelectedTxExplorer(
      [
        DEFAULT_TX_EXPLORER,
        { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
      ],
      "mempool",
      "mempool-space"
    )

    expect(selected.id).toBe("mempool")
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

  it("falls back to mempool.space when no local explorer is available", () => {
    const selected = resolveSelectedTxExplorer(PUBLIC_TX_EXPLORERS, null, "mempool-space")

    expect(selected.id).toBe("mempool-space")
  })

  it("uses mempool.space when multiple local explorers exist and no preference exists", () => {
    const selected = resolveSelectedTxExplorer(
      [
        DEFAULT_TX_EXPLORER,
        { id: "mempool", name: "Mempool", baseUrl: "http://umbrel.local:3006", isLocal: true },
        { id: "bitfeed", name: "Bitfeed", baseUrl: "http://umbrel.local:8314", isLocal: true },
      ],
      null,
      "mempool-space"
    )

    expect(selected.id).toBe("mempool-space")
  })

  it("builds transaction URLs with a shared /tx path", () => {
    expect(buildTransactionExplorerUrl("http://umbrel.local:3006/", "abc123")).toBe(
      "http://umbrel.local:3006/tx/abc123"
    )
  })

  it("resolves and builds custom transaction explorer URL templates", () => {
    const selected = resolveSelectedTxExplorer(
      PUBLIC_TX_EXPLORERS,
      encodeCustomTxExplorerPreference("https://example.com/transaction/{txid}"),
      "mempool-space",
      "Localized custom explorer"
    )

    expect(selected).toMatchObject({
      id: "custom",
      name: "Localized custom explorer",
      baseUrl: "https://example.com/transaction/{txid}",
      isCustom: true,
    })
    expect(buildTransactionExplorerUrl(selected.baseUrl, "abc123")).toBe(
      "https://example.com/transaction/abc123"
    )
  })
})
