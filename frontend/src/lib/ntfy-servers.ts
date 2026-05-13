import type { AppConfigResponse } from "@/lib/api"

export const PUBLIC_NTFY_SERVER_URL = "https://ntfy.sh"

export interface NtfyServerOption {
  id: string
  name: string
  baseUrl: string
  isLocal: boolean
  platform?: string
}

export const PUBLIC_NTFY_SERVER: NtfyServerOption = {
  id: "ntfy-sh",
  name: "ntfy",
  baseUrl: PUBLIC_NTFY_SERVER_URL,
  isLocal: false,
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, "")
}

export function buildNtfyServerOptions(config: AppConfigResponse): NtfyServerOption[] {
  const localServers = config.ntfy_servers
    .map((server) => ({
      id: server.id,
      name: server.name,
      baseUrl: normalizeBaseUrl(server.base_url),
      isLocal: true,
      platform: server.platform ?? undefined,
    }))
    .filter((server) => server.baseUrl.length > 0)

  return [PUBLIC_NTFY_SERVER, ...localServers]
}

export function resolveSelectedNtfyServer(
  options: NtfyServerOption[],
  savedServerUrl: string | null | undefined,
  defaultServerId: string | null | undefined
): NtfyServerOption {
  const normalizedSavedUrl = savedServerUrl ? normalizeBaseUrl(savedServerUrl) : null

  if (normalizedSavedUrl) {
    const matchingOption = options.find(
      (option) => normalizeBaseUrl(option.baseUrl) === normalizedSavedUrl
    )
    if (matchingOption) {
      return matchingOption
    }
    return { ...PUBLIC_NTFY_SERVER, baseUrl: normalizedSavedUrl }
  }

  const localServers = options.filter((option) => option.isLocal)
  if (localServers.length === 1) {
    // First-run self-hosted UX: a single detected local server wins until the user saves a public/custom URL.
    // If multiple local integrations are added later, choose an explicit default instead of guessing.
    return localServers[0]
  }

  const configDefaultServer = options.find((option) => option.id === defaultServerId)
  if (configDefaultServer) {
    return configDefaultServer
  }

  return PUBLIC_NTFY_SERVER
}

export function isBrowserSafeNtfyUrl(baseUrl: string): boolean {
  try {
    const parsed = new URL(baseUrl)
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return false
    }
    // Single-label names like umbrel are treated as browser-safe; Docker-internal
    // names with underscores, like ntfy_app_1, are invalid under RFC 1123 and hidden.
    return !parsed.hostname.includes("_")
  } catch {
    return false
  }
}
