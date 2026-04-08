import type { AppConfigResponse, TxExplorerConfig } from "@/lib/api"

export interface TxExplorerOption {
  id: string
  name: string
  baseUrl: string
  isLocal: boolean
}

export const DEFAULT_TX_EXPLORER: TxExplorerOption = {
  id: "mempool-space",
  name: "Mempool Space",
  baseUrl: "https://mempool.space",
  isLocal: false,
}

interface LocationLike {
  protocol: string
  hostname: string
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, "")
}

export function resolveExplorerBaseUrl(
  explorer: TxExplorerConfig,
  location: LocationLike | null
): string | null {
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
  const options = [DEFAULT_TX_EXPLORER]

  for (const explorer of config.tx_explorers) {
    const baseUrl = resolveExplorerBaseUrl(explorer, location)
    if (!baseUrl) continue

    options.push({
      id: explorer.id,
      name: explorer.name,
      baseUrl,
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
