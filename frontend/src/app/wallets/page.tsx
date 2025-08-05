"use client"

import { AlertCircle } from "lucide-react"
import Image from "next/image"
import { useWalletsList } from "@/hooks/useWalletsList"
import { formatBitcoinAmount } from "@/lib/utils"
import { useAuth } from "@/contexts/auth-context"
import { WalletCards } from "@/components/wallet-cards"
import { WalletOnboarding } from "@/components/wallet-onboarding"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

export default function WalletsPage() {
  const { wallets, error, lastUpdate, isConnected } = useWalletsList()
  const { isAuthenticated, isLoading } = useAuth()

  const handleWalletDeleted = () => {
    // Wallet list will auto-refresh via polling
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
    <>
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
      
      {wallets.length === 0 ? (
        <WalletOnboarding onCreateWallet={() => {}} />
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
    </>
  )
}
