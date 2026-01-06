"use client"

import { useState, useEffect, lazy, Suspense } from "react"
import { useParams, useRouter } from "next/navigation"
import { Transactions } from "@/components/transactions"
import { InlineWalletNameEdit } from "@/components/inline-wallet-name-edit"
import { WalletContactsList } from "@/components/wallet-contacts-list"
import { ContactModal } from "@/components/contact-modal"
import { DeleteWalletModal } from "@/components/delete-wallet-modal"
import { BalanceAlertsList } from "@/components/balance-alerts-list"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { ArrowLeft, Trash2, AlertCircle, Plus, AlertTriangle } from "lucide-react"
import Link from "next/link"
import { useWalletDetail } from "@/hooks/useWalletDetail"
import { useWalletsContext } from "@/contexts/wallets-context"
import { formatBitcoinAmount, hasReachedContactLimit } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

// Lazy load PlansModal to avoid bundling it when not needed
const PlansModal = lazy(() => import("@/components/plans-modal").then(mod => ({ default: mod.PlansModal })))



export default function WalletDetailPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  const { isAuthenticated, isLoading: authLoading, user, billingStatus, isCloudMode, isSelfHostedMode } = useAuth()
  const t = useTranslations('wallets')
  const tCommon = useTranslations('common')
  
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isAddContactModalOpen, setIsAddContactModalOpen] = useState(false)
  const [isUpgradeModalOpen, setIsUpgradeModalOpen] = useState(false)

  // Redirect unauthenticated users to sign-in when in cloud mode
  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [isCloudMode, isAuthenticated, authLoading, router])

  // Get wallet detail data directly using checksum
  const { wallet, transactions, contacts, balanceAlerts, error, isLoading, isConnected, lastUpdate, refresh } = useWalletDetail(checksum)
  
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

  const handleNameUpdated = (newName: string) => {
    // Immediately update local state for responsive UI 
    if (wallet) {
      wallet.name = newName
    }
    handleWalletUpdated()
  }

  const handleDeleteWallet = async (checksum: string) => {
    await api.deleteWallet(checksum)
    router.push('/wallets')
  }

  const handleAddContact = () => {
    // In self-hosted mode, no limits - always allow adding contacts
    if (isSelfHostedMode) {
      setIsAddContactModalOpen(true)
      return
    }
    
    // Check contact limits before opening create modal - use billing status as authoritative source
    const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
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
          <p className="mt-4 text-gray-600">{tCommon('loading')}</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in cloud mode
  if (isCloudMode && !isAuthenticated) {
    return null
  }

  if (isLoading && !wallet) {
    return (
      <div className="space-y-6">
        {/* Header Skeleton */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3 flex-wrap min-w-0">
              <div className="min-w-0">
                <nav className="flex items-center text-2xl text-muted-foreground min-w-0">
                  <Link href="/wallets" className="hover:text-foreground font-semibold flex-shrink-0">
                    {t('title')}
                  </Link>
                  <span className="mx-2 flex-shrink-0">/</span>
                  <Skeleton className="h-8 w-48" />
                </nav>
              </div>
            </div>
          </div>
        </section>

        {/* Main Content Skeleton */}
        <section>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
            {/* Wallet Info Sidebar Skeleton */}
            <div className="lg:col-span-1">
              <Card>
                <CardContent className="space-y-6">
                  <div>
                    <div className="text-sm font-medium text-muted-foreground mb-2">{t('detail.balance')}</div>
                    <Skeleton className="h-8 w-40" />
                    <Skeleton className="h-4 w-24 mt-1" />
                  </div>

                  <div className="pt-2 border-t">
                    <div className="flex items-center justify-between mb-2">
                      <div className="text-sm font-medium text-muted-foreground">{t('detail.contacts')}</div>
                    </div>
                    <Skeleton className="h-20 w-full" />
                  </div>

                  <div className="pt-2 border-t">
                    <Skeleton className="h-16 w-full" />
                  </div>
                </CardContent>
              </Card>
            </div>

            {/* Transactions Skeleton */}
            <div className="lg:col-span-2">
              <Card>
                <CardContent className="space-y-4 py-6">
                  <Skeleton className="h-6 w-32 mb-4" />
                  {[1, 2, 3].map((i) => (
                    <div key={i} className="space-y-2 pb-4 border-b last:border-0">
                      <div className="flex items-center justify-between">
                        <Skeleton className="h-5 w-24" />
                        <Skeleton className="h-5 w-32" />
                      </div>
                      <Skeleton className="h-4 w-full" />
                    </div>
                  ))}
                </CardContent>
              </Card>
            </div>
          </div>
        </section>
      </div>
    )
  }

  // Only show error if we have no cached data
  if (error && !wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon('backToWallets')}
            </Button>
          </Link>
        </div>

        <Alert variant="destructive">
          <AlertTitle>{t('error.title')}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      </>
    )
  }

  if (!wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon('backToWallets')}
            </Button>
          </Link>
        </div>

        <Alert>
          <AlertTitle>{t('detail.notFound.title')}</AlertTitle>
          <AlertDescription>
            {t('detail.notFound.description', { checksum })}
          </AlertDescription>
        </Alert>
      </>
    )
  }

  // If wallet is pending, show syncing message
  if (wallet.status === 'pending') {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              {tCommon('backToWallets')}
            </Button>
          </Link>
        </div>

        <Alert className="border-blue-200 bg-blue-50">
          <AlertCircle className="h-4 w-4 text-blue-600" />
          <AlertTitle className="text-blue-700">{t('detail.syncing.title')}</AlertTitle>
          <AlertDescription className="text-blue-600">
            {t('detail.syncing.description', { name: wallet.name })}
            <span className="block mt-2">
              {t('detail.syncing.returnPrompt')}
            </span>
            <div className="mt-3">
              <Link href="/wallets">
                <Button size="sm" variant="outline" className="border-blue-600 text-blue-600 hover:bg-blue-50">
                  {tCommon('backToWallets')}
                </Button>
              </Link>
            </div>
          </AlertDescription>
        </Alert>
      </>
    )
  }

  return (
    <>
      {/* Connection Warning Banner */}
      {(!isConnected || (error && wallet)) && (
        <Alert variant="destructive" className="mb-6">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>{t('connectionLost.title')}</AlertTitle>
          <AlertDescription>
            {t('connectionLost.description')}
            {lastUpdate && (
              <span className="block mt-1 text-xs">
                {t('connectionLost.lastUpdated', { time: new Date(lastUpdate * 1000).toLocaleString() })}
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Inactive Wallet Warning Banner - only in cloud mode */}
      {isCloudMode && wallet && wallet.is_active === false && (
        <Alert className="mb-6 border-orange-200 bg-orange-50">
          <AlertTriangle className="h-4 w-4 text-orange-600" />
          <AlertTitle className="text-orange-700">{t('detail.inactive.title')}</AlertTitle>
          <AlertDescription className="text-orange-600">
            {billingStatus?.subscription_status === 'expired'
              ? t('detail.inactive.descriptionExpired')
              : t('detail.inactive.descriptionTierLimit')}{' '}
            {t('detail.inactive.outdatedWarning')}
            <span className="block mt-2">
              <Link href="/subscription">
                <Button size="sm" className="bg-orange-600 hover:bg-orange-700 text-white">
                  {t('detail.inactive.upgradePlan')}
                </Button>
              </Link>
            </span>
          </AlertDescription>
        </Alert>
      )}
      
      <div className="space-y-6">
        {/* Header Section */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3 flex-wrap min-w-0">
              <div className="min-w-0">
                <nav className="flex items-center text-2xl text-muted-foreground min-w-0">
                  <Link href="/wallets" className="hover:text-foreground font-semibold flex-shrink-0">
                    {t('title')}
                  </Link>
                  <span className="mx-2 flex-shrink-0">/</span>
                  <div className="text-foreground font-semibold min-w-0">
                    <InlineWalletNameEdit
                      walletChecksum={wallet.checksum}
                      currentName={wallet.name}
                      onNameUpdated={handleNameUpdated}
                    />
                  </div>
                </nav>
              </div>
            </div>
          </div>
        </section>

        {/* Main Content */}
        <section>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Wallet Info Sidebar */}
        <div className="lg:col-span-1">
          <Card>
            <CardContent className="space-y-6">
              <div>
                <div className="text-sm font-medium text-muted-foreground mb-2">{t('detail.balance')}</div>
                <div className="text-2xl font-bold font-mono">
                  {formatBitcoinAmount(wallet.balance_total || 0)}
                </div>
                {wallet.balance_fiat !== undefined && wallet.fiat_currency && (
                  <div className="text-sm text-muted-foreground mt-1">
                    {new Intl.NumberFormat(undefined, {
                      style: 'currency',
                      currency: wallet.fiat_currency,
                      minimumFractionDigits: 0,
                      maximumFractionDigits: 0
                    }).format(wallet.balance_fiat)}
                  </div>
                )}
              </div>

              <div className="pt-2 border-t">
                <div className="flex items-center justify-between mb-2">
                  <div className="text-sm font-medium text-muted-foreground">{t('detail.contacts')}</div>
                  {!(isCloudMode && user?.is_admin) && !user?.is_demo && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={handleAddContact}
                      className="h-6 px-2 text-xs gap-1"
                    >
                      <Plus className="h-3 w-3" />
                      {tCommon('new')}
                    </Button>
                  )}
                </div>
                <WalletContactsList
                  walletChecksum={wallet.checksum}
                  contacts={contacts}
                  onContactsUpdated={handleWalletUpdated}
                  isWalletActive={wallet.is_active !== false}
                />
              </div>

              <div className="pt-2 border-t">
                <BalanceAlertsList
                  walletChecksum={wallet.checksum}
                  balanceAlerts={balanceAlerts}
                />
              </div>

              {!(isCloudMode && user?.is_admin) && !user?.is_demo && (
                <div className="pt-2 border-t flex justify-end">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setIsDeleteModalOpen(true)}
                    className="text-muted-foreground hover:text-red-600"
                  >
                    <Trash2 size={16} />
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </div>

        {/* Transaction Events */}
        <div className="lg:col-span-2">
          <Transactions
            transactions={transactions}
            isConnected={isConnected}
            error={error}
            lastUpdate={lastUpdate}
            walletsCount={1} // Single wallet context
          />
        </div>
      </div>
        </section>
      </div>

      <ContactModal
        key="add-contact"
        isOpen={isAddContactModalOpen}
        onClose={() => setIsAddContactModalOpen(false)}
        walletChecksum={wallet.checksum}
        onContactSaved={() => {
          setIsAddContactModalOpen(false)
          handleWalletUpdated()
        }}
      />

      <DeleteWalletModal
        wallet={wallet}
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        onConfirmDelete={() => handleDeleteWallet(wallet.checksum)}
      />

      <Suspense fallback={null}>
        <PlansModal
          isOpen={isUpgradeModalOpen}
          onClose={() => setIsUpgradeModalOpen(false)}
          currentTier={billingStatus?.subscription_tier || user?.subscription_tier || 'personal'}
          currentWalletCount={1} // We're on a single wallet page
          currentContactCount={contacts?.length || 0}
          limitType="contacts" // Show that we're upgrading for contacts
          billingStatus={billingStatus ? {
            subscription_status: billingStatus.subscription_status,
            stripe_customer_id: billingStatus.stripe_customer_id
          } : undefined}
        />
      </Suspense>

    </>
  )
}
