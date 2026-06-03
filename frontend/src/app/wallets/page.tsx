"use client"

import { AlertCircle } from "lucide-react"
import { useRouter } from "next/navigation"
import { useEffect } from "react"
import { useAuth } from "@/contexts/auth-context"
import { WalletCards } from "@/components/wallet-cards"
import { WalletOnboarding } from "@/components/wallet-onboarding"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { useWalletsContext } from "@/contexts/wallets-context"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import { cn } from "@/lib/utils"

export default function WalletsPage() {
  const t = useTranslations('wallets')
  const tCommon = useTranslations('common')
  const { wallets, error, lastUpdate, isConnected, isLoading: walletsLoading, refetchWallets } = useWalletsContext()
  const { isAuthenticated, isLoading: authLoading, user, isCloudMode, billingStatus } = useAuth()
  const router = useRouter()
  const { formatBitcoinAmount, formatFiatAmount, locale } = useFormatters()
  const walletLimit = billingStatus?.limits?.max_wallets ?? null
  const showWalletUsage = isCloudMode && walletLimit !== null && walletLimit !== -1
  const walletUsageExceeded = walletLimit !== null && walletLimit !== -1 && wallets.length > walletLimit

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

  // Redirect unauthenticated users to sign-in in all modes
  useEffect(() => {
    if (!authLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [isAuthenticated, authLoading, router])

  // Show loading spinner while auth or wallets are loading
  if (authLoading || walletsLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon('loading')}</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users
  if (!isAuthenticated) {
    return null
  }

  // Show dashboard for authenticated users
  return (
    <>
      {/* Connection Warning Banner */}
      {!isConnected && (
        <Alert variant="destructive" className="mb-6">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>{t('connectionLost.title')}</AlertTitle>
          <AlertDescription>
            {t('connectionLost.description')}
            {lastUpdate && (
              <span className="block mt-1 text-xs">
                {t('connectionLost.lastUpdated', { time: new Date(lastUpdate * 1000).toLocaleString(locale) })}
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}
      
      {wallets.length === 0 ? (
        <WalletOnboarding user={user} />
      ) : (
        <div className="space-y-8">
          {/* Wallet Cards Section */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-3 flex-wrap">
                <div>
                  <div className="flex items-center gap-2">
                    <h2 className="text-2xl font-semibold">{t('title')}</h2>
                    {showWalletUsage && (
                      <Badge
                        variant="outline"
                        className={cn(
                          "text-muted-foreground",
                          walletUsageExceeded && "border-orange-600 bg-orange-50 text-orange-700"
                        )}
                        aria-label={t('usage.wallets', { count: wallets.length, limit: walletLimit })}
                      >
                        {wallets.length} / {walletLimit}
                      </Badge>
                    )}
                  </div>
                  {wallets.length > 1 && (
                    <p className="text-sm text-muted-foreground">
                      {t('summary', { count: wallets.length, balance: formatBitcoinAmount(getTotalBalance()) })}
                      {(() => {
                        const fiatTotal = getTotalFiatBalance()
                        if (fiatTotal) {
                          return (
                            <span>
                              {' '}({formatFiatAmount(fiatTotal.amount, fiatTotal.currency)})
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
              subscriptionStatus={billingStatus?.subscription_status}
              onWalletDeleted={refetchWallets}
            />
          </section>

        </div>
      )}
    </>
  )
}
