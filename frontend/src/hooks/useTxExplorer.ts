"use client"

import { useEffect, useState } from "react"
import { api } from "@/lib/api"
import {
  DEFAULT_TX_EXPLORER,
  buildTxExplorerOptions,
  resolveSelectedTxExplorer,
  type TxExplorerOption,
} from "@/lib/tx-explorers"

const TX_EXPLORER_CHANGED_EVENT = "canary:tx-explorer-changed"

export function invalidateTxExplorerCache() {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(TX_EXPLORER_CHANGED_EVENT))
  }
}

async function resolveTxExplorer(): Promise<TxExplorerOption> {
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
    config.default_tx_explorer_id
  )
}

export function useTxExplorer(): TxExplorerOption {
  const [txExplorer, setTxExplorer] = useState<TxExplorerOption>(DEFAULT_TX_EXPLORER)

  useEffect(() => {
    let isMounted = true
    let requestVersion = 0

    const refreshTxExplorer = () => {
      requestVersion += 1
      const currentRequestVersion = requestVersion

      resolveTxExplorer()
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
  }, [])

  return txExplorer
}
