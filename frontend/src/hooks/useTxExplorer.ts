"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { api } from "@/lib/api"
import {
  DEFAULT_TX_EXPLORER,
  buildTxExplorerOptions,
  resolveSelectedTxExplorer,
  type TxExplorerOption,
} from "@/lib/tx-explorers"

const TX_EXPLORER_CHANGED_EVENT = "canary:tx-explorer-changed"
let inFlightExplorerRequest: Promise<TxExplorerOption> | null = null
let cachedTxExplorer: TxExplorerOption | null = null
let txExplorerCacheGeneration = 0

export function invalidateTxExplorerCache() {
  txExplorerCacheGeneration += 1
  inFlightExplorerRequest = null
  cachedTxExplorer = null

  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(TX_EXPLORER_CHANGED_EVENT))
  }
}

function getTxExplorerRequest(customExplorerName: string): Promise<TxExplorerOption> {
  if (cachedTxExplorer) {
    if (cachedTxExplorer.isCustom) {
      return Promise.resolve({ ...cachedTxExplorer, name: customExplorerName })
    }
    return Promise.resolve(cachedTxExplorer)
  }

  if (!inFlightExplorerRequest) {
    const requestGeneration = txExplorerCacheGeneration
    inFlightExplorerRequest = resolveTxExplorer(customExplorerName)
      .then((explorer) => {
        if (requestGeneration === txExplorerCacheGeneration) {
          cachedTxExplorer = explorer
        }
        return explorer
      })
      .finally(() => {
        inFlightExplorerRequest = null
      })
  }

  return inFlightExplorerRequest
}

async function resolveTxExplorer(customExplorerName: string): Promise<TxExplorerOption> {
  const [config, preferences] = await Promise.all([
    api.getConfig(),
    api.getUserPreferences().catch(() => null),
  ])

  const location = typeof window === "undefined"
    ? null
    : { protocol: window.location.protocol, hostname: window.location.hostname }

  const options = buildTxExplorerOptions(config, location)

  return resolveSelectedTxExplorer(
    options,
    preferences?.preferred_tx_explorer_id ?? null,
    config.default_tx_explorer_id,
    customExplorerName
  )
}

export function useTxExplorer(): TxExplorerOption {
  const tSettings = useTranslations("settings")
  const customExplorerName = tSettings("txExplorer.custom.title")
  const [txExplorer, setTxExplorer] = useState<TxExplorerOption>(DEFAULT_TX_EXPLORER)

  useEffect(() => {
    let isMounted = true
    let requestVersion = 0

    const refreshTxExplorer = () => {
      requestVersion += 1
      const currentRequestVersion = requestVersion

      getTxExplorerRequest(customExplorerName)
        .then((explorer) => {
          if (currentRequestVersion !== requestVersion) return
          if (isMounted) {
            setTxExplorer(explorer)
          }
        })
        .catch(() => {})
    }

    refreshTxExplorer()

    window.addEventListener(TX_EXPLORER_CHANGED_EVENT, refreshTxExplorer)

    return () => {
      isMounted = false
      window.removeEventListener(TX_EXPLORER_CHANGED_EVENT, refreshTxExplorer)
    }
  }, [customExplorerName])

  return txExplorer
}
