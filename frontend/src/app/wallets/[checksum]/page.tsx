"use client"

import { useState, useEffect, lazy, Suspense } from "react"
import { useParams, useRouter } from "next/navigation"
import { TransactionEvents } from "@/components/transaction-events"
import { InlineWalletNameEdit } from "@/components/inline-wallet-name-edit"
import { WalletContactsList } from "@/components/wallet-contacts-list"
import { ContactModal } from "@/components/contact-modal"
import { DeleteWalletModal } from "@/components/delete-wallet-modal"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { ArrowLeft, Trash2, AlertCircle, Plus, AlertTriangle } from "lucide-react"
import Link from "next/link"
import { useWalletDetail } from "@/hooks/useWalletDetail"
import { useWalletsContext } from "@/contexts/wallets-context"
import { formatBitcoinAmount, hasReachedContactLimit } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"

// Lazy load PlansModal to avoid bundling it when not needed
const PlansModal = lazy(() => import("@/components/plans-modal").then(mod => ({ default: mod.PlansModal })))



export default function WalletDetailPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  const { isAuthenticated, isLoading: authLoading, user, billingStatus, isSaasMode, isFossMode } = useAuth()
  
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isAddContactModalOpen, setIsAddContactModalOpen] = useState(false)
  const [isUpgradeModalOpen, setIsUpgradeModalOpen] = useState(false)

  // Redirect unauthenticated users to sign-in when in SAAS mode
  useEffect(() => {
    if (isSaasMode && !authLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [isSaasMode, isAuthenticated, authLoading, router])

  // Get wallet detail data directly using checksum
  const { wallet, events, contacts, error, isLoading, isConnected, lastUpdate, refresh } = useWalletDetail(checksum)
  
  // Share wallet data with layout context for SVG loading
  const { setCurrentWallet } = useWalletsContext()
  
  // Update context when wallet data changes
  useEffect(() => {
    if (setCurrentWallet) {
      setCurrentWallet(wallet)
    }
  }, [wallet, setCurrentWallet])
  

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
    // In FOSS mode, no limits - always allow adding contacts
    if (isFossMode) {
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
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in SAAS mode
  if (isSaasMode && !isAuthenticated) {
    return null
  }

  if (isLoading && !wallet) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading wallet...</p>
        </div>
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
              Back to Wallets
            </Button>
          </Link>
        </div>
        
        <Alert variant="destructive">
          <AlertTitle>Error</AlertTitle>
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
              Back to Wallets
            </Button>
          </Link>
        </div>
        
        <Alert>
          <AlertTitle>Wallet Not Found</AlertTitle>
          <AlertDescription>
            The wallet with checksum #{checksum} could not be found.
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
              Back to Wallets
            </Button>
          </Link>
        </div>
        
        <Alert className="border-blue-200 bg-blue-50">
          <AlertCircle className="h-4 w-4 text-blue-600" />
          <AlertTitle className="text-blue-700">Wallet Still Syncing</AlertTitle>
          <AlertDescription className="text-blue-600">
            <strong>{wallet.name}</strong> is still syncing and scanning for historical transactions. 
            This process can take a few minutes for wallets with transaction history.
            <span className="block mt-2">
              Please return to the wallets page and wait for the syncing to complete.
            </span>
            <div className="mt-3">
              <Link href="/wallets">
                <Button size="sm" variant="outline" className="border-blue-600 text-blue-600 hover:bg-blue-50">
                  Back to Wallets
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
          <AlertTitle>Backend Connection Lost</AlertTitle>
          <AlertDescription>
            Unable to connect to the backend service. Displaying cached data.
            {lastUpdate && (
              <span className="block mt-1 text-xs">
                Last updated: {new Date(lastUpdate * 1000).toLocaleString()}
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Inactive Wallet Warning Banner - only in SAAS mode */}
      {isSaasMode && wallet && wallet.is_active === false && (
        <Alert className="mb-6 border-orange-200 bg-orange-50">
          <AlertTriangle className="h-4 w-4 text-orange-600" />
          <AlertTitle className="text-orange-700">Wallet Inactive</AlertTitle>
          <AlertDescription className="text-orange-600">
            This wallet exceeds your subscription tier limits and won&apos;t sync automatically. 
            Transaction history and balance shown may be outdated.
            <span className="block mt-2">
              <Link href="/settings/subscription">
                <Button size="sm" className="bg-orange-600 hover:bg-orange-700 text-white">
                  Upgrade Plan
                </Button>
              </Link>
            </span>
          </AlertDescription>
        </Alert>
      )}
      
      <div className="space-y-8">
        {/* Header Section */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3 flex-wrap">
              <div>
                <nav className="flex items-center text-2xl text-muted-foreground">
                  <Link href="/wallets" className="hover:text-foreground font-semibold">
                    Wallets
                  </Link>
                  <span className="mx-2">/</span>
                  <div className="text-foreground font-semibold">
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
            <CardContent className="space-y-4">
              <div>
                <div className="text-sm text-muted-foreground">Balance</div>
                <div className="text-2xl font-bold font-mono">
                  {formatBitcoinAmount(wallet.balance_total || 0)}
                </div>
              </div>
              

              <div className="pt-2 border-t">
                <div className="text-sm text-muted-foreground mb-2">Contacts</div>
                <WalletContactsList 
                  walletChecksum={wallet.checksum}
                  contacts={contacts}
                  onContactsUpdated={handleWalletUpdated}
                  isWalletActive={wallet.is_active !== false}
                />
                {!(isSaasMode && user?.is_admin) && (
                  <div className="mt-3">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={handleAddContact}
                      className="h-8 gap-1 w-full"
                    >
                      <Plus size={14} />
                      Add Contact
                    </Button>
                  </div>
                )}
              </div>

              {!(isSaasMode && user?.is_admin) && (
                <div className="pt-4 border-t flex justify-end">
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
          <TransactionEvents 
            events={events}
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