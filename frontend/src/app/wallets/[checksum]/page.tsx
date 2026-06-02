"use client"

import { useState, useEffect, lazy, Suspense } from "react"
import { useParams, useRouter } from "next/navigation"
import { Transactions } from "@/components/transactions"
import { ContactModal } from "@/components/contact-modal"
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
import { hasReachedContactLimit } from "@/lib/utils"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

// Lazy load PlansModal to avoid bundling it when not needed
const PlansModal = lazy(() =>
  import("@/components/plans-modal").then((mod) => ({ default: mod.PlansModal }))
)

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
    isSelfHostedMode,
  } = useAuth()
  const t = useTranslations("wallets")
  const tCommon = useTranslations("common")

  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isAddContactModalOpen, setIsAddContactModalOpen] = useState(false)
  const [isUpgradeModalOpen, setIsUpgradeModalOpen] = useState(false)

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
    contacts,
    balanceAlerts,
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

  const handleAddContact = () => {
    // In self-hosted mode, no limits - always allow adding contacts
    if (isSelfHostedMode) {
      setIsAddContactModalOpen(true)
      return
    }

    // Check contact limits before opening create modal
    const currentTier =
      billingStatus?.subscription_tier || user?.subscription_tier || "personal"
    if (hasReachedContactLimit(contacts?.length || 0, currentTier)) {
      setIsUpgradeModalOpen(true)
      return
    }

    setIsAddContactModalOpen(true)
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
    onDeleteWallet: wallet ? () => handleDeleteWallet(wallet.checksum) : undefined,
  })
  if (errorState) {
    return errorState
  }

  // At this point, wallet is guaranteed to exist and not be pending

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
              contacts={contacts || []}
              balanceAlerts={balanceAlerts || []}
              onAddContact={handleAddContact}
              onContactsUpdated={handleWalletUpdated}
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

      <ContactModal
        key="add-contact"
        isOpen={isAddContactModalOpen}
        onClose={() => setIsAddContactModalOpen(false)}
        walletChecksum={wallet!.checksum}
        onContactSaved={() => {
          setIsAddContactModalOpen(false)
          handleWalletUpdated()
        }}
      />

      <DeleteWalletModal
        wallet={wallet!}
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        onConfirmDelete={() => handleDeleteWallet(wallet!.checksum)}
      />

      <Suspense fallback={null}>
        <PlansModal
          isOpen={isUpgradeModalOpen}
          onClose={() => setIsUpgradeModalOpen(false)}
          currentTier={
            billingStatus?.subscription_tier || user?.subscription_tier || "personal"
          }
          currentWalletCount={1}
          currentContactCount={contacts?.length || 0}
          limitType="contacts"
          billingStatus={
            billingStatus
              ? {
                  subscription_status: billingStatus.subscription_status,
                  stripe_customer_id: billingStatus.stripe_customer_id,
                }
              : undefined
          }
        />
      </Suspense>
    </>
  )
}
