"use client"

import { useState, useEffect } from "react"
import { api } from "@/lib/api"

const DEFAULT_MEMPOOL_URL = "https://mempool.space"

let cachedMempoolUrl: string | null = null
let fetchPromise: Promise<string> | null = null

async function resolveMempoolUrl(): Promise<string> {
  try {
    const config = await api.getConfig()

    if (config.mempool_url) {
      // Full URL provided (e.g., CANARY_MEMPOOL_URL=http://mempool.local:3006)
      return config.mempool_url.replace(/\/$/, "")
    }

    if (config.mempool_port && typeof window !== "undefined") {
      // Port-only (Umbrel auto-detection): use browser's hostname + port
      return `${window.location.protocol}//${window.location.hostname}:${config.mempool_port}`
    }
  } catch {
    // Fall through to default on any error
  }

  return DEFAULT_MEMPOOL_URL
}

export function useMempoolUrl(): string {
  const [mempoolUrl, setMempoolUrl] = useState<string>(cachedMempoolUrl ?? DEFAULT_MEMPOOL_URL)

  useEffect(() => {
    if (cachedMempoolUrl) {
      setMempoolUrl(cachedMempoolUrl)
      return
    }

    if (!fetchPromise) {
      fetchPromise = resolveMempoolUrl()
    }

    fetchPromise.then((url) => {
      cachedMempoolUrl = url
      setMempoolUrl(url)
    })
  }, [])

  return mempoolUrl
}
