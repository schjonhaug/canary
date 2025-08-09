"use client"

import { AlertCircle } from "lucide-react"
import { useRouter } from "next/navigation"
import { useEffect } from "react"
import { formatBitcoinAmount } from "@/lib/utils"
import { useAuth } from "@/contexts/auth-context"
import { WalletCards } from "@/components/wallet-cards"
import { WalletOnboarding } from "@/components/wallet-onboarding"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { useWalletsContext } from "@/contexts/wallets-context"

export default function WalletsPage() {
  const { wallets, error, lastUpdate, isConnected, onAddWallet } = useWalletsContext()
  const { isAuthenticated, isLoading } = useAuth()
  const router = useRouter()

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + wallet.balance_total
    }, 0)
  }

  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED === 'true'

  // Redirect unauthenticated users to sign-in when auth is enabled
  useEffect(() => {
    if (authEnabled && !isLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [authEnabled, isAuthenticated, isLoading, router])

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

  // Return null while redirecting unauthenticated users
  if (authEnabled && !isAuthenticated) {
    return null
  }

  // Show dashboard for authenticated users or when auth is disabled
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
        <WalletOnboarding onAddWallet={onAddWallet} />
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
              wallets={wallets}
              error={error}
              lastUpdate={lastUpdate}
            />
          </section>

        </div>
      )}
    </>
  )
}
