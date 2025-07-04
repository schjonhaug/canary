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

interface WalletCardsProps {
  selectedWalletId: number | null
  onSelectWallet: (walletId: number | null) => void
}

export function WalletCards({ selectedWalletId, onSelectWallet }: WalletCardsProps) {
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

  const extractChecksum = (descriptor: string) => {
    const checksumMatch = descriptor.match(/#([a-zA-Z0-9]+)$/)
    return checksumMatch ? checksumMatch[1] : "Unknown"
  }

  const handleWalletClick = (walletId: number) => {
    if (selectedWalletId === walletId) {
      onSelectWallet(null) // Deselect if already selected
    } else {
      onSelectWallet(walletId)
    }
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
        {wallets.map((wallet) => {
          const isSelected = selectedWalletId === wallet.id
          return (
            <Card 
              key={wallet.id} 
              className={`cursor-pointer transition-all duration-200 ${
                isSelected 
                  ? "ring-2 ring-orange-500 bg-orange-50/50 shadow-lg" 
                  : "hover:shadow-md hover:bg-gray-50/50"
              }`}
              onClick={() => handleWalletClick(wallet.id)}
            >
              <CardHeader className="pb-3 relative">
                <CardTitle className="text-lg truncate pr-20" title={wallet.name}>
                  {wallet.name}
                </CardTitle>
                <div className="absolute top-6 right-6 text-xs font-mono text-muted-foreground bg-gray-100 px-2 py-1 rounded">
                  #{extractChecksum(wallet.descriptor)}
                </div>
                <CardDescription className="text-xs text-muted-foreground">
                  Click to {isSelected ? 'deselect' : 'view transactions'}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <div className="text-sm text-muted-foreground">Balance</div>
                    <div className={`text-xl font-bold font-mono ${
                      isSelected ? "text-orange-700" : ""
                    }`}>
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
          )
        })}
      </div>
    </div>
  )
}