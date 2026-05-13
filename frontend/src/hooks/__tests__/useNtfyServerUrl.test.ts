import { renderHook, waitFor } from "@testing-library/react"
import { normalizeNtfyUrl, useNtfyServerTarget } from "../useNtfyServerUrl"
import { UMBREL_NTFY_SERVER_ID } from "@/lib/ntfy-servers"

jest.mock("@/lib/api", () => ({
  api: {
    getUserPreferences: jest.fn(),
    getConfig: jest.fn(),
  },
}))

const mockApi = jest.requireMock("@/lib/api").api

describe('normalizeNtfyUrl', () => {
  it('returns null for empty string', () => {
    expect(normalizeNtfyUrl('')).toBeNull()
    expect(normalizeNtfyUrl('  ')).toBeNull()
  })

  it('accepts valid https URLs', () => {
    expect(normalizeNtfyUrl('https://ntfy.example.com')).toBe('https://ntfy.example.com')
  })

  it('accepts valid http URLs', () => {
    expect(normalizeNtfyUrl('http://ntfy.local:8080')).toBe('http://ntfy.local:8080')
  })

  it("accepts Docker-internal http hostnames", () => {
    expect(normalizeNtfyUrl("http://ntfy_app_1")).toBe("http://ntfy_app_1")
  })

  it('strips trailing slashes', () => {
    expect(normalizeNtfyUrl('https://ntfy.example.com/')).toBe('https://ntfy.example.com')
    expect(normalizeNtfyUrl('https://ntfy.example.com///')).toBe('https://ntfy.example.com')
  })

  it('prepends https:// when no scheme is present', () => {
    expect(normalizeNtfyUrl('ntfy.example.com')).toBe('https://ntfy.example.com')
  })

  it('rejects javascript: URLs', () => {
    expect(normalizeNtfyUrl('javascript:alert(1)')).toBeNull()
  })

  it('rejects data: URLs', () => {
    expect(normalizeNtfyUrl('data:text/html,<script>alert(1)</script>')).toBeNull()
  })

  it('rejects ftp: URLs', () => {
    expect(normalizeNtfyUrl('ftp://ntfy.example.com')).toBeNull()
  })

  it('preserves path components', () => {
    expect(normalizeNtfyUrl('https://example.com/ntfy')).toBe('https://example.com/ntfy')
  })

  it('preserves port numbers', () => {
    expect(normalizeNtfyUrl('https://ntfy.local:8443')).toBe('https://ntfy.local:8443')
  })
})

describe("useNtfyServerTarget", () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getUserPreferences.mockResolvedValue({
      ntfy_server_url: null,
    })
  })

  it("marks browser-reachable local ntfy servers as safe", async () => {
    mockApi.getConfig.mockResolvedValue({
      tx_explorers: [],
      default_tx_explorer_id: "mempool-space",
      ntfy_servers: [
        {
          id: UMBREL_NTFY_SERVER_ID,
          name: "ntfy",
          base_url: "http://umbrel",
          platform: "umbrel",
        },
      ],
      default_ntfy_server_id: UMBREL_NTFY_SERVER_ID,
    })

    const { result } = renderHook(() => useNtfyServerTarget())

    await waitFor(() => {
      expect(result.current).toEqual({ url: "http://umbrel", isBrowserSafe: true })
    })
  })

  it("marks Docker-internal local ntfy servers as unsafe for browser links", async () => {
    mockApi.getConfig.mockResolvedValue({
      tx_explorers: [],
      default_tx_explorer_id: "mempool-space",
      ntfy_servers: [
        {
          id: UMBREL_NTFY_SERVER_ID,
          name: "ntfy",
          base_url: "http://ntfy_app_1",
          platform: "umbrel",
        },
      ],
      default_ntfy_server_id: UMBREL_NTFY_SERVER_ID,
    })

    const { result } = renderHook(() => useNtfyServerTarget())

    await waitFor(() => {
      expect(result.current).toEqual({ url: "http://ntfy_app_1", isBrowserSafe: false })
    })
  })
})
