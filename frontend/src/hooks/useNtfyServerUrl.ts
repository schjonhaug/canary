import { useState, useEffect } from "react"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import {
  PUBLIC_NTFY_SERVER_URL,
  buildNtfyServerOptions,
  isBrowserSafeNtfyUrl,
  resolveSelectedNtfyServer,
} from "@/lib/ntfy-servers"

const DEFAULT_NTFY_URL = PUBLIC_NTFY_SERVER_URL
let inFlightNtfyTargetRequest: Promise<{ url: string; isBrowserSafe: boolean }> | null = null
let inFlightNtfyTargetRequestAuthState: boolean | null = null

/**
 * Validates and normalizes an ntfy server URL.
 * - Rejects non-http/https schemes (e.g. javascript:)
 * - Prepends https:// if no scheme is present
 * - Strips trailing slashes
 * - Returns null if the URL is invalid
 */
export function normalizeNtfyUrl(url: string): string | null {
  const trimmed = url.trim()
  if (!trimmed) return null

  // Reject URLs with non-http(s) schemes
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed) && !/^https?:\/\//i.test(trimmed)) {
    return null
  }

  // If no scheme, prepend https://
  const withScheme = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`

  try {
    const parsed = new URL(withScheme)
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null
    }
    return parsed.origin + parsed.pathname.replace(/\/+$/, "")
  } catch {
    return null
  }
}

/**
 * Hook that fetches the user's configured ntfy server URL from preferences.
 * Returns a validated, normalized URL that is safe to use in href attributes.
 * Falls back to https://ntfy.sh if no custom server is configured or on error.
 */
export function useNtfyServerTarget(): { url: string; isBrowserSafe: boolean } {
  const { isAuthenticated, isLoading } = useAuth()
  const [ntfyServerUrl, setNtfyServerUrl] = useState(DEFAULT_NTFY_URL)
  const [isBrowserSafe, setIsBrowserSafe] = useState(true)

  useEffect(() => {
    if (isLoading) return

    let cancelled = false

    getNtfyServerTargetRequest(isAuthenticated)
      .then((target) => {
        if (cancelled) return
        setNtfyServerUrl(target.url)
        setIsBrowserSafe(target.isBrowserSafe)
      })
      .catch(() => {
        // Fall back to default ntfy.sh
      })

    return () => {
      cancelled = true
    }
  }, [isAuthenticated, isLoading])

  return {
    url: ntfyServerUrl,
    isBrowserSafe,
  }
}

export function useNtfyServerUrl(): string {
  return useNtfyServerTarget().url
}

function getNtfyServerTargetRequest(
  isAuthenticated: boolean
): Promise<{ url: string; isBrowserSafe: boolean }> {
  if (!inFlightNtfyTargetRequest || inFlightNtfyTargetRequestAuthState !== isAuthenticated) {
    inFlightNtfyTargetRequestAuthState = isAuthenticated
    inFlightNtfyTargetRequest = resolveNtfyServerTarget(isAuthenticated).finally(() => {
      inFlightNtfyTargetRequest = null
      inFlightNtfyTargetRequestAuthState = null
    })
  }

  return inFlightNtfyTargetRequest
}

async function resolveNtfyServerTarget(
  isAuthenticated: boolean
): Promise<{ url: string; isBrowserSafe: boolean }> {
  const [config, prefs] = await Promise.all([
    api.getConfig(),
    isAuthenticated ? api.getUserPreferences().catch(() => null) : Promise.resolve(null),
  ])

  const selectedServer = resolveSelectedNtfyServer(
    buildNtfyServerOptions(config),
    prefs?.ntfy_server_url,
    config.default_ntfy_server_id
  )
  const normalized = normalizeNtfyUrl(selectedServer.baseUrl)

  if (!normalized) {
    return { url: DEFAULT_NTFY_URL, isBrowserSafe: true }
  }

  return {
    url: normalized,
    isBrowserSafe: isBrowserSafeNtfyUrl(normalized),
  }
}
