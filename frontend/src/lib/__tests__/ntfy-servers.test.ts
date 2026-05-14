import {
  PUBLIC_NTFY_SERVER,
  buildNtfyServerOptions,
  isBrowserSafeNtfyUrl,
  resolveSelectedNtfyServer,
} from "../ntfy-servers"

describe("ntfy server helpers", () => {
  const config = {
    tx_explorers: [],
    default_tx_explorer_id: "mempool-space",
    ntfy_servers: [
      {
        id: "umbrel-ntfy",
        name: "ntfy",
        base_url: "http://ntfy_app_1/",
        platform: "umbrel",
        default_topic: null,
        managed_auth: false,
      },
    ],
    default_ntfy_server_id: "umbrel-ntfy",
  }

  it("builds public/custom and local options", () => {
    expect(buildNtfyServerOptions(config)).toEqual([
      PUBLIC_NTFY_SERVER,
      {
        id: "umbrel-ntfy",
        name: "ntfy",
        baseUrl: "http://ntfy_app_1",
        isLocal: true,
        platform: "umbrel",
        managedAuth: false,
      },
    ])
  })

  it("uses saved public ntfy over detected local ntfy", () => {
    const selected = resolveSelectedNtfyServer(
      buildNtfyServerOptions(config),
      "https://ntfy.sh",
      "umbrel-ntfy"
    )

    expect(selected.id).toBe("ntfy-sh")
  })

  it("uses saved local ntfy when detected", () => {
    const selected = resolveSelectedNtfyServer(
      buildNtfyServerOptions(config),
      "http://ntfy_app_1",
      "umbrel-ntfy"
    )

    expect(selected.id).toBe("umbrel-ntfy")
  })

  it("selects a single detected local ntfy when there is no saved preference", () => {
    const selected = resolveSelectedNtfyServer(buildNtfyServerOptions(config), null, "umbrel-ntfy")

    expect(selected.id).toBe("umbrel-ntfy")
  })

  it("selects a single detected local ntfy when the saved preference is empty", () => {
    const selected = resolveSelectedNtfyServer(buildNtfyServerOptions(config), "", "umbrel-ntfy")

    expect(selected.id).toBe("umbrel-ntfy")
  })

  it("selects a single detected local ntfy before the public config default", () => {
    const selected = resolveSelectedNtfyServer(buildNtfyServerOptions(config), null, "ntfy-sh")

    expect(selected.id).toBe("umbrel-ntfy")
  })

  it("falls back to public ntfy when no local server is available", () => {
    const selected = resolveSelectedNtfyServer(
      buildNtfyServerOptions({
        tx_explorers: [],
        default_tx_explorer_id: "mempool-space",
        ntfy_servers: [],
        default_ntfy_server_id: "ntfy-sh",
      }),
      null,
      "ntfy-sh"
    )

    expect(selected.id).toBe("ntfy-sh")
  })

  it("uses the editable public option for unknown saved URLs", () => {
    const selected = resolveSelectedNtfyServer(
      buildNtfyServerOptions(config),
      "https://ntfy.example.com",
      "umbrel-ntfy"
    )

    expect(selected).toMatchObject({
      id: "ntfy-sh",
      baseUrl: "https://ntfy.example.com",
    })
  })

  it("rejects Docker-internal URLs but allows browser-reachable single-label hosts", () => {
    expect(isBrowserSafeNtfyUrl("http://ntfy_app_1")).toBe(false)
    expect(isBrowserSafeNtfyUrl("http://umbrel")).toBe(true)
    expect(isBrowserSafeNtfyUrl("http://ntfy")).toBe(true)
    expect(isBrowserSafeNtfyUrl("https://ntfy.sh")).toBe(true)
    expect(isBrowserSafeNtfyUrl("http://localhost:8080")).toBe(true)
    expect(isBrowserSafeNtfyUrl("http://192.168.1.10:8080")).toBe(true)
    expect(isBrowserSafeNtfyUrl("http://valid-host/path_with_underscore")).toBe(true)
  })

  it("filters blank detected ntfy server URLs", () => {
    const options = buildNtfyServerOptions({
      tx_explorers: [],
      default_tx_explorer_id: "mempool-space",
      ntfy_servers: [
        {
          id: "blank-ntfy",
          name: "ntfy",
          base_url: "   ",
          platform: "umbrel",
          default_topic: null,
          managed_auth: false,
        },
      ],
      default_ntfy_server_id: "blank-ntfy",
    })

    expect(options).toEqual([PUBLIC_NTFY_SERVER])
  })

  it("preserves managed auth and default topic metadata", () => {
    const options = buildNtfyServerOptions({
      tx_explorers: [],
      default_tx_explorer_id: "mempool-space",
      ntfy_servers: [
        {
          id: "startos-ntfy",
          name: "ntfy",
          base_url: "http://ntfy.startos",
          platform: "startos",
          default_topic: "canary",
          managed_auth: true,
        },
      ],
      default_ntfy_server_id: "startos-ntfy",
    })

    expect(options[1]).toMatchObject({
      id: "startos-ntfy",
      baseUrl: "http://ntfy.startos",
      defaultTopic: "canary",
      managedAuth: true,
    })
  })
})
