"use client"

import { useState } from "react"
import { usePathname } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { DemoBanner } from "@/components/demo-banner"
import { useWalletsList } from "@/hooks/useWalletsList"
import { Wallet } from "@/types"
import { WalletsContext } from "@/contexts/wallets-context"

export default function WalletsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [currentWallet, setCurrentWallet] = useState<Wallet | null>(null)
  const pathname = usePathname()

  // Check if we're on the add wallet page (including sub-routes like /wallets/add/sparrow)
  const isAddWalletPage = pathname.startsWith('/wallets/add')

  // Fetch wallets list on the main wallets page and add wallet pages (for limit checking)
  const shouldFetchWallets = pathname === '/wallets' || isAddWalletPage
  const { wallets, error, lastUpdate, isConnected, isLoading, refresh: refetchWallets, addWallet } = useWalletsList(shouldFetchWallets)

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <DemoBanner />
      <AppHeader />

      {/* Pass wallet data to children via React context */}
      <WalletsContext.Provider value={{ wallets, error, lastUpdate, isConnected, isLoading, currentWallet, setCurrentWallet, addWallet, refetchWallets }}>
        {children}
      </WalletsContext.Provider>

      <AppFooter />
    </div>
  )
}
