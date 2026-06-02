"use client"

import { useEffect, useMemo, useState, memo } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Users, AlertTriangle, Loader2, Trash2 } from "lucide-react"
import Link from "next/link"
import { loadWalletSvg, getCachedWalletSvg, formatDateTime } from "@/lib/utils"
import { formatRelativeTime, parseWalletTimestampToUnix } from "@/lib/wallet-time"
import { isStalePendingWallet } from "@/lib/wallet-status"
import { api } from "@/lib/api"
import { useLocale, useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"

import { Wallet } from "../types"

// Component for loading SVG with synchronous cache to prevent flickering
const WalletIcon = memo(({ wallet }: { wallet: Wallet }) => {
  const cachedSvg = getCachedWalletSvg(wallet.hex_color, wallet.wallet_type)
  const [asyncSvg, setAsyncSvg] = useState<string>('')

  useEffect(() => {
    if (cachedSvg) {
      return
    }

    let isMounted = true

    loadWalletSvg(wallet.hex_color, wallet.wallet_type)
      .then(content => { if (isMounted) setAsyncSvg(content) })
      .catch(() => {})

    return () => { isMounted = false }
  }, [wallet.hex_color, wallet.wallet_type, cachedSvg])

  return (
    <div
      className="w-6 h-6 flex-shrink-0"
      role="img"
      aria-label={wallet.wallet_type === 'address' ? 'Address wallet' : 'Descriptor wallet'}
      dangerouslySetInnerHTML={{ __html: cachedSvg || asyncSvg }}
    />
  )
})

WalletIcon.displayName = 'WalletIcon'

const LastSyncedText = memo(function LastSyncedText({
  wallet,
  now,
  className = "",
}: {
  wallet: Wallet
  now: number
  className?: string
}) {
  const t = useTranslations('wallets')
  const locale = useLocale()

  if (!wallet.last_synced_at) {
    return null
  }

  const lastSyncedUnix = parseWalletTimestampToUnix(wallet.last_synced_at)
  const fallbackTime = formatDateTime(wallet.last_synced_at, locale)
  const lastSyncedTime = lastSyncedUnix !== undefined
    ? formatRelativeTime(lastSyncedUnix, locale, now)
    : fallbackTime
  const lastSyncedTitle = lastSyncedUnix !== undefined
    ? formatDateTime(lastSyncedUnix, locale)
    : fallbackTime

  const hasValidFallback = fallbackTime !== 'Invalid date'
  if (lastSyncedUnix === undefined && !hasValidFallback) {
    return null
  }

  return (
    <span className={className} title={lastSyncedTitle}>
      {t('card.lastSynced', { time: lastSyncedTime })}
    </span>
  )
})

LastSyncedText.displayName = 'LastSyncedText'

interface WalletCardsProps {
  wallets: Wallet[]
  error: string | null
  lastUpdate: number | null
  subscriptionStatus?: string
  onWalletDeleted?: () => void
}

export function WalletCards({ wallets, error, lastUpdate, subscriptionStatus, onWalletDeleted }: WalletCardsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [relativeTimeNow, setRelativeTimeNow] = useState(() => Date.now())
  const [deletingWallet, setDeletingWallet] = useState<string | null>(null)
  const t = useTranslations('wallets')
  const tCommon = useTranslations('common')
  const { formatBitcoinAmount, formatFiatAmount, locale } = useFormatters()
  const hasTimeSensitiveWallet = useMemo(
    () => wallets.some(wallet => wallet.last_synced_at || (wallet.status === 'pending' && !wallet.last_synced_at)),
    [wallets]
  )
  const sortedWallets = useMemo(
    () => [...wallets].sort((a, b) => a.name.localeCompare(b.name, locale)),
    [wallets, locale]
  )

  // Check if subscription is expired
  const isSubscriptionExpired = subscriptionStatus === 'expired'

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  useEffect(() => {
    if (!hasTimeSensitiveWallet) {
      return
    }

    const interval = setInterval(() => setRelativeTimeNow(Date.now()), 30000)
    return () => clearInterval(interval)
  }, [hasTimeSensitiveWallet])

  const handleDeleteWallet = async (checksum: string) => {
    setDeletingWallet(checksum)
    try {
      await api.deleteWallet(checksum)
      onWalletDeleted?.()
    } finally {
      setDeletingWallet(null)
    }
  }

  if (!hasReceivedData) {
    return (
      <div className="space-y-4">
        {/* Individual Wallet Card Skeletons */}
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <Card key={i} className="hover:shadow-md hover:bg-gray-50/50">
              <CardHeader className="pb-3 relative">
                <div className="flex items-center justify-between">
                  <Skeleton className="h-6 w-32" />
                  <div className="flex items-center gap-2">
                    <Skeleton className="h-8 w-8 rounded-md" />
                    <Skeleton className="h-6 w-16" />
                  </div>
                </div>
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <Skeleton className="h-4 w-16 mb-1" />
                    <Skeleton className="h-6 w-40" />
                  </div>
                  <div className="flex justify-between items-center">
                    <Skeleton className="h-3 w-32" />
                    <div className="flex items-center gap-1">
                      <Skeleton className="h-3 w-3" />
                      <Skeleton className="h-3 w-4" />
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    )
  }

  if (error && wallets.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('error.title')}</CardTitle>
          <CardDescription className="text-destructive">
            {t('error.loadFailed', { error })}
          </CardDescription>
        </CardHeader>
      </Card>
    )
  }

  if (wallets.length === 0) {
    return (
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <CardTitle>{t('empty.title')}</CardTitle>
            <CardDescription>
              {t('empty.description')}
            </CardDescription>
          </CardHeader>
        </Card>

      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Individual Wallet Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {sortedWallets.map((wallet) => {
          const isInactive = wallet.is_active === false
          const isSyncing = wallet.status === 'pending'
          const isStalePending = isStalePendingWallet(wallet, relativeTimeNow)
          const isFailed = wallet.status === 'failed'
          const canRecover = isFailed || isStalePending
          
          // If wallet is syncing normally, render non-clickable card with spinner
          if (isSyncing && !isStalePending) {
            return (
              <Card key={wallet.checksum} className="transition-all duration-200">
                <CardHeader className="pb-3">
                  <div className="flex items-center gap-2 min-w-0">
                    <WalletIcon wallet={wallet} />
                    <CardTitle className="text-lg truncate min-w-0" title={wallet.name}>
                      {wallet.name}
                    </CardTitle>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-col items-center justify-center pt-2 pb-6">
                    <Loader2 className="h-10 w-10 animate-spin text-muted-foreground" />
                    <span
                      id={`wallet-sync-status-${wallet.checksum}`}
                      className="text-sm font-medium text-foreground mt-3"
                    >
                      {t('card.syncing')}
                    </span>
                    <span className="text-xs text-muted-foreground mt-1 text-center">
                      {t('card.syncingDescription')}
                    </span>
                    <div
                      className="mt-4 h-2 w-full max-w-48 overflow-hidden rounded-md bg-muted"
                      role="progressbar"
                      aria-labelledby={`wallet-sync-status-${wallet.checksum}`}
                    >
                      <div className="h-full w-full animate-pulse rounded-md bg-primary" />
                    </div>
                    <LastSyncedText
                      wallet={wallet}
                      now={relativeTimeNow}
                      className="text-xs text-muted-foreground mt-3"
                    />
                  </div>
                </CardContent>
              </Card>
            )
          }

          if (canRecover) {
            const title = isFailed ? t('card.failed') : t('card.stuck')
            const description = isFailed ? t('card.failedDescription') : t('card.stuckDescription')

            return (
              <Card key={wallet.checksum} className="transition-all duration-200 border-orange-200 bg-orange-50/50">
                <CardHeader className="pb-3">
                  <div className="flex items-center gap-2 min-w-0">
                    <WalletIcon wallet={wallet} />
                    <CardTitle className="text-lg truncate min-w-0" title={wallet.name}>
                      {wallet.name}
                    </CardTitle>
                  </div>
                  <Badge variant="outline" className="text-xs text-orange-700 border-orange-600 bg-orange-50 w-fit">
                    <AlertTriangle className="h-3 w-3 mr-1" />
                    {title}
                  </Badge>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <p className="text-sm text-orange-700">{description}</p>
                    <Button
                      variant="destructive"
                      size="sm"
                      className="gap-2"
                      onClick={() => handleDeleteWallet(wallet.checksum)}
                      disabled={deletingWallet === wallet.checksum}
                    >
                      <Trash2 className="h-4 w-4" />
                      {deletingWallet === wallet.checksum ? tCommon('deleting') : tCommon('delete')}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            )
          }
          
          // Render normal clickable card
          return (
            <Link key={wallet.checksum} href={`/wallets/${wallet.checksum}`} prefetch={true}>
              <Card className={`transition-all duration-200 hover:shadow-md hover:bg-muted/50 cursor-pointer ${
                isInactive ? 'opacity-60 border-orange-200 bg-orange-50/50' : ''
              }`}>
                <CardHeader className="pb-3">
                  <div className="flex items-center gap-2 min-w-0">
                    <WalletIcon wallet={wallet} />
                    <CardTitle className={`text-lg truncate min-w-0 ${isInactive ? 'text-muted-foreground line-through' : ''}`} title={wallet.name}>
                      {wallet.name}
                    </CardTitle>
                  </div>
                  {isInactive && (
                    <Badge variant="outline" className="text-xs text-orange-600 border-orange-600 bg-orange-50 w-fit">
                      <AlertTriangle className="h-3 w-3 mr-1" />
                      {t('card.inactive')}
                    </Badge>
                  )}
                  {isInactive && (
                    <CardDescription className="text-xs text-orange-600 mt-1">
                      {isSubscriptionExpired
                        ? t('card.inactiveExpired')
                        : t('card.inactiveTierLimit')}
                    </CardDescription>
                  )}
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    <div>
                      <div className="text-sm text-muted-foreground">{t('card.balance')}</div>
                      <div className={`text-xl font-bold font-mono ${isInactive ? 'text-muted-foreground' : ''}`}>
                        {formatBitcoinAmount(wallet.balance_total || 0)}
                      </div>
                      {wallet.balance_fiat !== undefined && wallet.fiat_currency && (
                        <div className="text-sm text-muted-foreground mt-1">
                          {formatFiatAmount(wallet.balance_fiat, wallet.fiat_currency)}
                        </div>
                      )}
                    </div>
                    <div className="flex justify-between items-center text-xs text-muted-foreground">
                      <span>
                        {wallet.last_activity
                          ? t('card.lastActivity', { date: new Date(parseInt(wallet.last_activity) * 1000).toLocaleDateString(locale, {
                              year: '2-digit',
                              month: '2-digit',
                              day: '2-digit'
                            })})
                          : t('card.noRecentActivity')
                        }
                      </span>
                      <div className="flex items-center gap-1">
                        <Users className="h-3 w-3" />
                        <span>{wallet.contact_count || 0}</span>
                      </div>
                    </div>
                    <LastSyncedText
                      wallet={wallet}
                      now={relativeTimeNow}
                      className="block text-xs text-muted-foreground"
                    />
                  </div>
                </CardContent>
              </Card>
            </Link>
          )
        })}
      </div>

    </div>
  )
}
