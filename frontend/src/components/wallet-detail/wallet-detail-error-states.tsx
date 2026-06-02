"use client"

import { ReactNode } from "react"
import Link from "next/link"
import { ArrowLeft, AlertCircle, Trash2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { isStalePendingWallet } from "@/lib/wallet-status"
import type { Wallet } from "@/types"

interface ErrorStateParams {
  error: string | null
  wallet: Wallet | null
  checksum: string
  t: (key: string, params?: Record<string, string>) => string
  tCommon: (key: string) => string
  canDelete?: boolean
  onDeleteWallet?: () => void
}

/**
 * Returns a full-page error state component if an error condition exists,
 * or null if the wallet is ready to display normally.
 */
export function getWalletDetailErrorState({
  error,
  wallet,
  checksum,
  t,
  tCommon,
  canDelete = false,
  onDeleteWallet,
}: ErrorStateParams): ReactNode | null {
  // Error with no cached wallet data
  if (error && !wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon("backToWallets")}
            </Button>
          </Link>
        </div>

        <Alert variant="destructive">
          <AlertTitle>{t("error.title")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      </>
    )
  }

  // Wallet not found
  if (!wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon("backToWallets")}
            </Button>
          </Link>
        </div>

        <Alert>
          <AlertTitle>{t("detail.notFound.title")}</AlertTitle>
          <AlertDescription>
            {t("detail.notFound.description", { checksum })}
          </AlertDescription>
        </Alert>
      </>
    )
  }

  const isStalePending = isStalePendingWallet(wallet)
  const isFailed = wallet.status === "failed"

  if (isFailed || isStalePending) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon("backToWallets")}
            </Button>
          </Link>
        </div>

        <Alert className="border-orange-200 bg-orange-50">
          <AlertCircle className="h-4 w-4 text-orange-600" />
          <AlertTitle className="text-orange-700">
            {isFailed ? t("detail.failed.title") : t("detail.stuck.title")}
          </AlertTitle>
          <AlertDescription className="text-orange-700">
            {isFailed
              ? t("detail.failed.description", { name: wallet.name })
              : t("detail.stuck.description", { name: wallet.name })}
            <div className="mt-3 flex flex-wrap gap-2">
              {canDelete && onDeleteWallet && (
                <Button
                  size="sm"
                  variant="destructive"
                  className="gap-2"
                  onClick={onDeleteWallet}
                >
                  <Trash2 className="h-4 w-4" />
                  {tCommon("delete")}
                </Button>
              )}
              <Link href="/wallets">
                <Button
                  size="sm"
                  variant="outline"
                  className="border-orange-600 text-orange-700 hover:bg-orange-50"
                >
                  {tCommon("backToWallets")}
                </Button>
              </Link>
            </div>
          </AlertDescription>
        </Alert>
      </>
    )
  }

  // Wallet is still syncing
  if (wallet.status === "pending") {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon("backToWallets")}
            </Button>
          </Link>
        </div>

        <Alert className="border-blue-200 bg-blue-50">
          <AlertCircle className="h-4 w-4 text-blue-600" />
          <AlertTitle className="text-blue-700">
            {t("detail.syncing.title")}
          </AlertTitle>
          <AlertDescription className="text-blue-600">
            {t("detail.syncing.description", { name: wallet.name })}
            <span id={`wallet-detail-sync-status-${wallet.checksum}`} className="block mt-2">
              {t("detail.syncing.returnPrompt")}
            </span>
            <div
              className="mt-4 h-2 w-full max-w-sm overflow-hidden rounded-md bg-blue-100"
              role="progressbar"
              aria-labelledby={`wallet-detail-sync-status-${wallet.checksum}`}
            >
              <div className="h-full w-full animate-pulse rounded-md bg-blue-600" />
            </div>
            <div className="mt-3">
              <Link href="/wallets">
                <Button
                  size="sm"
                  variant="outline"
                  className="border-blue-600 text-blue-600 hover:bg-blue-50"
                >
                  {tCommon("backToWallets")}
                </Button>
              </Link>
            </div>
          </AlertDescription>
        </Alert>
      </>
    )
  }

  // No error state - wallet is ready
  return null
}
