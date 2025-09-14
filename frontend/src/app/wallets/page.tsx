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
  const { isAuthenticated, isLoading, user, isSaasMode } = useAuth()
  const router = useRouter()

  // Set page title
  useEffect(() => {
    document.title = "Canary - Wallets"
  }, [])

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + wallet.balance_total
    }, 0)
  }

  const getTotalFiatBalance = () => {
    // Only calculate if all wallets have fiat values and same currency
    const firstCurrency = wallets[0]?.fiat_currency
    if (!firstCurrency) return null
    
    const allSameCurrency = wallets.every(w => w.fiat_currency === firstCurrency)
    if (!allSameCurrency) return null
    
    const total = wallets.reduce((sum, wallet) => {
      return sum + (wallet.balance_fiat || 0)
    }, 0)
    
    return { amount: total, currency: firstCurrency }
  }

  // Redirect unauthenticated users to sign-in when in SAAS mode
  useEffect(() => {
    if (isSaasMode && !isLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [isSaasMode, isAuthenticated, isLoading, router])

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

  // Return null while redirecting unauthenticated users in SAAS mode
  if (isSaasMode && !isAuthenticated) {
    return null
  }

  // Show dashboard for authenticated users or in FOSS mode
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
        <WalletOnboarding onAddWallet={onAddWallet} user={user} />
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
                      {(() => {
                        const fiatTotal = getTotalFiatBalance()
                        if (fiatTotal) {
                          return (
                            <span>
                              {' '}(
                              {new Intl.NumberFormat(undefined, {
                                style: 'currency',
                                currency: fiatTotal.currency,
                                minimumFractionDigits: 0,
                                maximumFractionDigits: 0
                              }).format(fiatTotal.amount)}
                              )
                            </span>
                          )
                        }
                        return null
                      })()}
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
