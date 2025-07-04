"use client"

import { useEffect, useState } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"

interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  created_at: string
  balance_total?: number
  last_activity?: string
}

export function WalletCards() {
  const [wallets, setWallets] = useState<Wallet[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    async function fetchWallets() {
      try {
        // Use the same hostname as the current page, but port 3000 for the API
        const apiUrl = `http://${window.location.hostname}:3000/wallets`
        const response = await fetch(apiUrl)
        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`)
        }
        const data = await response.json()
        setWallets(data)
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to fetch wallets")
      } finally {
        setLoading(false)
      }
    }

    fetchWallets()
    
    // Refresh every 10 seconds (less frequent than transaction events)
    const interval = setInterval(fetchWallets, 10000)
    return () => clearInterval(interval)
  }, [])

  const formatBalance = (sats?: number) => {
    if (sats === undefined || sats === null) return "0,00000000 BTC"
    const btc = sats / 100_000_000
    return `${btc.toLocaleString("nb-NO", { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
  }

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + (wallet.balance_total || 0)
    }, 0)
  }

  if (loading) {
    return (
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle>Loading...</CardTitle>
            <CardDescription>Fetching wallet data...</CardDescription>
          </CardHeader>
        </Card>
      </div>
    )
  }

  if (error) {
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
      <Card>
        <CardHeader>
          <CardTitle>No Wallets</CardTitle>
          <CardDescription>
            No wallets found. Create a wallet to get started.
          </CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <div className="space-y-4">
      {/* Total Balance Summary Card */}
      <Card className="bg-gradient-to-r from-orange-50 to-amber-50 border-orange-200">
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2">
            ₿ Total Balance
            <Badge variant="outline" className="text-xs">
              {wallets.length} wallet{wallets.length !== 1 ? 's' : ''}
            </Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-3xl font-bold font-mono text-orange-800">
            {formatBalance(getTotalBalance())}
          </div>
        </CardContent>
      </Card>

      {/* Individual Wallet Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {wallets.map((wallet) => (
          <Card key={wallet.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-3">
              <CardTitle className="text-lg truncate" title={wallet.name}>
                {wallet.name}
              </CardTitle>
              <CardDescription className="text-xs font-mono truncate" title={wallet.descriptor}>
                {wallet.descriptor.length > 50 
                  ? `${wallet.descriptor.substring(0, 47)}...` 
                  : wallet.descriptor
                }
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                <div>
                  <div className="text-sm text-muted-foreground">Balance</div>
                  <div className="text-xl font-bold font-mono">
                    {formatBalance(wallet.balance_total)}
                  </div>
                </div>
                <div className="flex justify-center items-center text-xs text-muted-foreground">
                  <span>
                    {wallet.last_activity 
                      ? `Last activity: ${new Date(wallet.last_activity).toLocaleDateString()}` 
                      : "No recent activity"
                    }
                  </span>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}