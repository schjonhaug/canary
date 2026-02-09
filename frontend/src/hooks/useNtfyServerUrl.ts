import { useState, useEffect } from "react"
import { api } from "@/lib/api"

const DEFAULT_NTFY_URL = "https://ntfy.sh"

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
export function useNtfyServerUrl(): string {
  const [ntfyServerUrl, setNtfyServerUrl] = useState(DEFAULT_NTFY_URL)

  useEffect(() => {
    let cancelled = false

    api.getUserPreferences()
      .then((prefs) => {
        if (cancelled) return
        if (prefs.ntfy_server_url) {
          const normalized = normalizeNtfyUrl(prefs.ntfy_server_url)
          if (normalized) {
            setNtfyServerUrl(normalized)
          }
        }
      })
      .catch(() => {
        // Fall back to default ntfy.sh
      })

    return () => { cancelled = true }
  }, [])

  return ntfyServerUrl
}
