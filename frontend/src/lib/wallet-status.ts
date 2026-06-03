import { parseWalletTimestampToUnix } from "@/lib/wallet-time"
import type { Wallet } from "@/types"

export const STALE_PENDING_WALLET_MS = 10 * 60 * 1000

export function isStalePendingWallet(wallet: Wallet, now = Date.now()) {
  if (wallet.status !== "pending" || wallet.last_synced_at) {
    return false
  }

  const createdAt = parseWalletTimestampToUnix(wallet.created_at)
  if (createdAt === undefined) {
    return false
  }

  return now - createdAt * 1000 >= STALE_PENDING_WALLET_MS
}

export function isRecoverableWallet(wallet: Wallet, now = Date.now()) {
  return wallet.status === "failed" || isStalePendingWallet(wallet, now)
}
