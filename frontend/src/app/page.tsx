"use client"

import { useState, lazy, Suspense } from "react"
import { WalletCards } from "@/components/wallet-cards"
import { AppFooter } from "@/components/app-footer"
import { AppHeader } from "@/components/app-header"
import { AlertCircle } from "lucide-react"
import Image from "next/image"
import { useWalletsList } from "@/hooks/useWalletsList"
import { formatBitcoinAmount } from "@/lib/utils"
import { useAuth } from "@/contexts/auth-context"
import { WalletOnboarding } from "@/components/wallet-onboarding"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

// Lazy load modal components for code splitting
const CreateWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))

export default function Home() {
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const { wallets, error, lastUpdate, isConnected, refresh } = useWalletsList()
  const { isAuthenticated, isLoading } = useAuth()


  const handleCreateWallet = () => {
    setIsCreateWalletOpen(true)
  }

  const handleWalletCreated = () => {
    setIsCreateWalletOpen(false)
    refresh() // Immediately refresh dashboard after wallet creation
  }

  const handleWalletDeleted = () => {
    refresh() // Immediately refresh dashboard after wallet deletion
  }

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + wallet.balance_total
    }, 0)
  }

  // Show landing page for logged out users
  if (!isLoading && !isAuthenticated) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={120}
            height={120}
            className="mx-auto mb-6"
          />
          <h1 className="text-4xl font-bold tracking-wide">Canary</h1>
        </div>
      </div>
    )
  }

  // Show loading state while auth is loading
  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // Show dashboard for authenticated users
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Connection Warning Banner */}
      {!isConnected && (
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
      <AppHeader 
        showCreateWallet={wallets.length > 0}
        onCreateWallet={handleCreateWallet}
      />
      
      {wallets.length === 0 ? (
        <WalletOnboarding onCreateWallet={handleCreateWallet} />
      ) : (
        <div className="mt-8 space-y-8">
          {/* Wallet Cards Section */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-3 flex-wrap">
                <div>
                  <h2 className="text-2xl font-semibold">Wallets</h2>
                  {wallets.length > 1 && (
                    <p className="text-sm text-muted-foreground">
                      Tracking {wallets.length} wallets with a total balance of {formatBitcoinAmount(getTotalBalance())}
                    </p>
                  )}
                </div>
              </div>
            </div>
            <WalletCards 
              selectedWalletId={null}
              onSelectWallet={() => {}}
              wallets={wallets}
              isConnected={isConnected}
              error={error}
              lastUpdate={lastUpdate}
              onWalletDeleted={handleWalletDeleted}
            />
          </section>

        </div>
      )}
      
      
      <Suspense fallback={null}>
        <CreateWalletModal
          isOpen={isCreateWalletOpen}
          onClose={() => setIsCreateWalletOpen(false)}
          onWalletCreated={handleWalletCreated}
        />
      </Suspense>
      
      <AppFooter />
    </div>
  )
}
