"use client"

import { useState, useEffect } from "react"
import { useParams, useRouter } from "next/navigation"
import { Transactions } from "@/components/transactions"
import { DeleteWalletModal } from "@/components/delete-wallet-modal"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import {
  WalletDetailSkeleton,
  getWalletDetailErrorState,
  WalletDetailWarningBanners,
  WalletDetailHeader,
  WalletInfoSidebar,
} from "@/components/wallet-detail"
import { useWalletDetail } from "@/hooks/useWalletDetail"
import { useWalletsContext } from "@/contexts/wallets-context"
import { getTranslatedApiError } from "@/lib/utils"
import { api, ApiError } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

export default function WalletDetailPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  const {
    isAuthenticated,
    isLoading: authLoading,
    user,
    billingStatus,
    isCloudMode,
  } = useAuth()
  const t = useTranslations("wallets")
  const tCommon = useTranslations("common")
  const tApiErrors = useTranslations("errors.api")

  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isRecoveryDeleting, setIsRecoveryDeleting] = useState(false)
  const [recoveryDeleteError, setRecoveryDeleteError] = useState<string | null>(null)
  const [relativeTimeNow, setRelativeTimeNow] = useState(() => Date.now())

  // Redirect unauthenticated users to sign-in when in cloud mode
  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) {
      router.push("/sign-in")
    }
  }, [isCloudMode, isAuthenticated, authLoading, router])

  // Get wallet detail data directly using checksum
  const {
    wallet,
    transactions,
    error,
    isLoading,
    isLoadingMore,
    isConnected,
    hasMoreTransactions,
    lastUpdate,
    transactionNotifications,
    loadingTransactionNotifications,
    transactionNotificationErrors,
    loadTransactionNotifications,
    refresh,
    loadMoreTransactions,
  } = useWalletDetail(checksum)

  // Share wallet data with layout context for SVG loading
  const { setCurrentWallet } = useWalletsContext()

  // Update context when wallet data changes
  useEffect(() => {
    if (setCurrentWallet) {
      setCurrentWallet(wallet)
    }
  }, [wallet, setCurrentWallet])

  // Set page title with wallet name
  useEffect(() => {
    if (wallet?.name) {
      document.title = `Canary - ${wallet.name}`
    }
  }, [wallet?.name])

  useEffect(() => {
    // Only stale pending recovery depends on elapsed time; failed wallets are
    // recoverable immediately and do not need a timer.
    if (wallet?.status !== "pending" || wallet.last_synced_at) {
      return
    }

    const interval = setInterval(() => setRelativeTimeNow(Date.now()), 30000)
    return () => clearInterval(interval)
  }, [wallet?.last_synced_at, wallet?.status])

  const handleWalletUpdated = () => {
    refresh()
  }

  const handleNameUpdated = () => {
    // Name was updated on backend by child component, refresh to get new data
    handleWalletUpdated()
  }

  const handleDeleteWallet = async (walletChecksum: string) => {
    await api.deleteWallet(walletChecksum)
    router.push("/wallets")
  }

  const handleRecoveryDeleteWallet = async (walletChecksum: string) => {
    if (isRecoveryDeleting) {
      return
    }

    setIsRecoveryDeleting(true)
    setRecoveryDeleteError(null)

    try {
      await handleDeleteWallet(walletChecksum)
    } catch (err) {
      setRecoveryDeleteError(
        err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("delete.failed")
      )
    } finally {
      setIsRecoveryDeleting(false)
    }
  }

  // Show loading state while auth is loading
  if (authLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon("loading")}</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in cloud mode
  if (isCloudMode && !isAuthenticated) {
    return null
  }

  // Show skeleton while loading
  if (isLoading && !wallet) {
    return <WalletDetailSkeleton />
  }

  // Check for error states (error, not found, pending sync)
  const showActions = !(isCloudMode && user?.is_admin) && !user?.is_demo
  const errorState = getWalletDetailErrorState({
    error,
    wallet,
    checksum,
    t,
    tCommon,
    canDelete: showActions,
    onDeleteWallet: wallet ? () => handleRecoveryDeleteWallet(wallet.checksum) : undefined,
    isDeleting: isRecoveryDeleting,
    deleteError: recoveryDeleteError,
    now: relativeTimeNow,
  })
  if (errorState) {
    return errorState
  }

  return (
    <>
      <WalletDetailWarningBanners
        wallet={wallet!}
        isConnected={isConnected}
        error={error}
        lastUpdate={lastUpdate}
        isCloudMode={isCloudMode}
        billingStatus={billingStatus}
        t={t}
      />

      <div className="space-y-6">
        <WalletDetailHeader
          walletChecksum={wallet!.checksum}
          walletName={wallet!.name}
          onNameUpdated={handleNameUpdated}
        />

        {/* Main Content */}
        <section>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
            <WalletInfoSidebar
              wallet={wallet!}
              onDeleteClick={() => setIsDeleteModalOpen(true)}
              showActions={showActions}
            />

            {/* Transaction Events */}
            <div className="lg:col-span-2">
              <Transactions
                selectedWalletChecksum={wallet?.checksum}
                transactions={transactions}
                error={error}
                lastUpdate={lastUpdate}
                hasMoreTransactions={hasMoreTransactions}
                isLoadingMore={isLoadingMore}
                onLoadMore={loadMoreTransactions}
                walletsCount={1}
                transactionNotifications={transactionNotifications}
                loadingTransactionNotifications={loadingTransactionNotifications}
                transactionNotificationErrors={transactionNotificationErrors}
                loadTransactionNotifications={loadTransactionNotifications}
              />
            </div>
          </div>
        </section>
      </div>

      <DeleteWalletModal
        wallet={wallet!}
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        onConfirmDelete={() => handleDeleteWallet(wallet!.checksum)}
      />
    </>
  )
}
