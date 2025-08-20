"use client"

import { useEffect, useState, useMemo, memo } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Badge } from "@/components/ui/badge"
import { Users, AlertTriangle } from "lucide-react"
import Link from "next/link"
import { loadCanarySvg, getCachedCanarySvg, formatBitcoinAmount, formatDateTime } from "@/lib/utils"

// Note: checksum is now directly available from wallet.checksum
import { Wallet } from "../types"

interface WalletCardsProps {
  wallets: Wallet[]
  error: string | null
  lastUpdate: number | null
}

export function WalletCards({ wallets, error, lastUpdate }: WalletCardsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  // Component for loading SVG with synchronous cache to prevent flickering
  const WalletIcon = memo(({ wallet }: { wallet: Wallet }) => {
    // Try to get cached SVG first (synchronous)
    const cachedSvg = getCachedCanarySvg(wallet.hex_color)
    const [svgContent, setSvgContent] = useState<string>(cachedSvg || '')
    
    useEffect(() => {
      // If we already have cached content, don't reload
      if (cachedSvg) {
        return
      }
      
      let isMounted = true
      
      loadCanarySvg(wallet.hex_color).then(content => {
        if (isMounted) {
          setSvgContent(content)
        }
      })
      
      return () => { isMounted = false }
    }, [wallet.hex_color, cachedSvg])
    
    const checksumTitle = useMemo(() => 
      `Checksum: #${wallet.checksum}`, 
      [wallet.checksum]
    )
    
    return (
      <div 
        className="w-6 h-6 cursor-help flex-shrink-0"
        title={checksumTitle}
        dangerouslySetInnerHTML={{ __html: svgContent }}
      />
    )
  })
  
  WalletIcon.displayName = 'WalletIcon'


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
          <CardTitle>Error</CardTitle>
          <CardDescription className="text-destructive">
            Failed to load wallets: {error}
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
            <CardTitle>No Wallets</CardTitle>
            <CardDescription>
              No wallets found. Use the &quot;Add Wallet&quot; button in the header to get started.
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
        {[...wallets].sort((a, b) => a.name.localeCompare(b.name)).map((wallet) => {
          const isInactive = wallet.is_active === false
          const isSyncing = wallet.balance_total === 0 && !wallet.last_activity
          
          // If wallet is syncing, render non-clickable card with skeleton content
          if (isSyncing) {
            return (
              <Card key={wallet.checksum} className="transition-all duration-200 border-blue-200 bg-blue-50/30">
                <CardHeader className="pb-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <WalletIcon wallet={wallet} />
                      <CardTitle className="text-lg truncate" title={wallet.name}>
                        {wallet.name}
                      </CardTitle>
                    </div>
                    <Badge variant="outline" className="text-xs text-blue-600 border-blue-600 bg-blue-50">
                      <div className="h-3 w-3 mr-1 animate-spin rounded-full border border-blue-600 border-t-transparent" />
                      Syncing...
                    </Badge>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    <div>
                      <div className="text-sm text-muted-foreground">Balance</div>
                      <div className="flex items-center gap-2">
                        <Skeleton className="h-6 w-32" />
                      </div>
                    </div>
                    <div className="flex justify-between items-center text-xs text-muted-foreground">
                      <Skeleton className="h-3 w-24" />
                      <div className="flex items-center gap-1">
                        <Users className="h-3 w-3" />
                        <span>{wallet.contact_count || 0}</span>
                      </div>
                    </div>
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
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <WalletIcon wallet={wallet} />
                      <CardTitle className={`text-lg truncate ${isInactive ? 'text-muted-foreground line-through' : ''}`} title={wallet.name}>
                        {wallet.name}
                      </CardTitle>
                    </div>
                    {isInactive && (
                      <Badge variant="outline" className="text-xs text-orange-600 border-orange-600 bg-orange-50">
                        <AlertTriangle className="h-3 w-3 mr-1" />
                        Inactive
                      </Badge>
                    )}
                  </div>
                  {isInactive && (
                    <CardDescription className="text-xs text-orange-600 mt-1">
                      This wallet exceeds your subscription tier limits and won&apos;t sync automatically
                    </CardDescription>
                  )}
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    <div>
                      <div className="text-sm text-muted-foreground">Balance</div>
                      <div className={`text-xl font-bold font-mono ${isInactive ? 'text-muted-foreground' : ''}`}>
                        {formatBitcoinAmount(wallet.balance_total || 0)}
                      </div>
                    </div>
                    <div className="flex justify-between items-center text-xs text-muted-foreground">
                      <span>
                        {wallet.last_activity 
                          ? `Last activity: ${formatDateTime(parseInt(wallet.last_activity))}` 
                          : "No recent activity"
                        }
                      </span>
                      <div className="flex items-center gap-1">
                        <Users className="h-3 w-3" />
                        <span>{wallet.contact_count || 0}</span>
                      </div>
                    </div>
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