import type { AppConfigResponse, TxExplorerConfig } from "@/lib/api"

export interface TxExplorerOption {
  id: string
  name: string
  baseUrl: string
  candidateUrls?: string[]
  isLocal: boolean
}

export const PUBLIC_TX_EXPLORERS: TxExplorerOption[] = [
  {
    id: "mempool-space",
    name: "Mempool Space",
    baseUrl: "https://mempool.space",
    isLocal: false,
  },
  {
    id: "bitfeed-public",
    name: "Bitfeed",
    baseUrl: "https://bitfeed.live",
    isLocal: false,
  },
  {
    id: "btc-rpc-explorer-public",
    name: "BTC RPC Explorer",
    baseUrl: "https://bitcoinexplorer.org",
    isLocal: false,
  },
]

export const DEFAULT_TX_EXPLORER: TxExplorerOption = PUBLIC_TX_EXPLORERS[0]

interface LocationLike {
  protocol: string
  hostname: string
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, "")
}

function isBrowserSafeUrl(baseUrl: string): boolean {
  try {
    const parsed = new URL(baseUrl)
    return parsed.protocol === "http:" || parsed.protocol === "https:"
  } catch {
    return false
  }
}

function uniqueNormalizedUrls(urls: string[]): string[] {
  const normalizedUrls: string[] = []

  for (const url of urls) {
    const normalizedUrl = normalizeBaseUrl(url)
    if (
      normalizedUrl &&
      isBrowserSafeUrl(normalizedUrl) &&
      !normalizedUrls.includes(normalizedUrl)
    ) {
      normalizedUrls.push(normalizedUrl)
    }
  }

  return normalizedUrls
}

function chooseBestCandidateUrl(
  candidateUrls: string[],
  location: LocationLike | null
): string | null {
  if (candidateUrls.length === 0) {
    return null
  }

  if (location) {
    const matchingUrl = candidateUrls.find((candidateUrl) => {
      try {
        return new URL(candidateUrl).hostname === location.hostname
      } catch {
        return false
      }
    })

    if (matchingUrl) {
      return matchingUrl
    }
  }

  return candidateUrls[0]
}

export function resolveExplorerBaseUrl(
  explorer: TxExplorerConfig,
  location: LocationLike | null
): string | null {
  const candidateUrls = uniqueNormalizedUrls(explorer.base_urls ?? [])
  const bestCandidateUrl = chooseBestCandidateUrl(candidateUrls, location)
  if (bestCandidateUrl) {
    return bestCandidateUrl
  }

  if (explorer.base_url) {
    return normalizeBaseUrl(explorer.base_url)
  }

  if (explorer.port && location) {
    return `${location.protocol}//${location.hostname}:${explorer.port}`
  }

  return null
}

export function buildTxExplorerOptions(
  config: AppConfigResponse,
  location: LocationLike | null
): TxExplorerOption[] {
  const options = [...PUBLIC_TX_EXPLORERS]

  for (const explorer of config.tx_explorers) {
    const candidateUrls = uniqueNormalizedUrls(explorer.base_urls ?? [])
    const baseUrl = resolveExplorerBaseUrl(explorer, location)
    if (!baseUrl) continue

    options.push({
      id: explorer.id,
      name: explorer.name,
      baseUrl,
      ...(candidateUrls.length > 0 ? { candidateUrls } : {}),
      isLocal: true,
    })
  }

  return options
}

export function resolveSelectedTxExplorer(
  options: TxExplorerOption[],
  preferredExplorerId: string | null | undefined,
  defaultExplorerId: string | null | undefined
): TxExplorerOption {
  const preferredExplorer = options.find((explorer) => explorer.id === preferredExplorerId)
  if (preferredExplorer) {
    return preferredExplorer
  }

  const localExplorers = options.filter((explorer) => explorer.isLocal)
  if (localExplorers.length === 1) {
    return localExplorers[0]
  }

  const configDefaultExplorer = options.find((explorer) => explorer.id === defaultExplorerId)
  if (configDefaultExplorer) {
    return configDefaultExplorer
  }

  return DEFAULT_TX_EXPLORER
}

export function buildTransactionExplorerUrl(baseUrl: string, txid: string): string {
  return `${normalizeBaseUrl(baseUrl)}/tx/${txid}`
}
