"use client"

import { useEffect, useState } from "react"
import { api } from "@/lib/api"
import {
  DEFAULT_TX_EXPLORER,
  buildTxExplorerOptions,
  resolveSelectedTxExplorer,
  type TxExplorerOption,
} from "@/lib/tx-explorers"

let cachedExplorer: TxExplorerOption | null = null
let fetchPromise: Promise<TxExplorerOption> | null = null

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
  const [txExplorer, setTxExplorer] = useState<TxExplorerOption>(cachedExplorer ?? DEFAULT_TX_EXPLORER)

  useEffect(() => {
    if (cachedExplorer) {
      setTxExplorer(cachedExplorer)
      return
    }

    if (!fetchPromise) {
      fetchPromise = resolveTxExplorer()
    }

    let isMounted = true

    fetchPromise
      .then((explorer) => {
        cachedExplorer = explorer
        if (isMounted) {
          setTxExplorer(explorer)
        }
      })
      .catch(() => {
        fetchPromise = null
      })

    return () => {
      isMounted = false
    }
  }, [])

  return txExplorer
}
