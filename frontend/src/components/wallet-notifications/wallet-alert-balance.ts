import type { Wallet } from "@/types"

export type WalletAlertBalance =
  | { status: "unavailable" }
  | { status: "ready"; sats: number; fiat?: number; fiatCurrency?: string }

export function walletAlertBalance(wallet: Wallet | null | undefined): WalletAlertBalance {
  if (!wallet || wallet.status === "failed" || wallet.status === "deleted") {
    return { status: "unavailable" }
  }
  if (wallet.status === "pending" && !wallet.last_synced_at) {
    return { status: "unavailable" }
  }
  return {
    status: "ready",
    sats: wallet.balance_total,
    fiat: wallet.balance_fiat,
    fiatCurrency: wallet.fiat_currency,
  }
}
