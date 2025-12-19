"use client"

import { useState, useEffect } from "react"
import { usePathname } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { DemoBanner } from "@/components/demo-banner"
import { useWalletsList } from "@/hooks/useWalletsList"
import { loadCanarySvg, getCachedCanarySvg } from "@/lib/utils"
import { Wallet } from "@/types"
import { WalletsContext } from "@/contexts/wallets-context"
import { useAuth } from "@/contexts/auth-context"

export default function WalletsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [walletSvg, setWalletSvg] = useState<string>("")
  const [currentWallet, setCurrentWallet] = useState<Wallet | null>(null)
  const pathname = usePathname()
  const { user, isCloudMode } = useAuth()

  // Check if we're on a wallet detail page
  const isWalletDetailPage = pathname.startsWith('/wallets/') && pathname !== '/wallets' && pathname !== '/wallets/add'

  // Check if we're on the add wallet page
  const isAddWalletPage = pathname === '/wallets/add'

  // Only fetch wallets list on the main wallets page, not on detail or add pages
  const shouldFetchWallets = pathname === '/wallets'
  const { wallets, error, lastUpdate, isConnected, isLoading, refresh: refetchWallets, addWallet } = useWalletsList(shouldFetchWallets)

  // Load SVG when current wallet data is available for detail pages
  useEffect(() => {
    if (isWalletDetailPage && currentWallet?.hex_color) {
      const cachedSvg = getCachedCanarySvg(currentWallet.hex_color)
      if (cachedSvg) {
        setWalletSvg(cachedSvg)
      } else {
        loadCanarySvg(currentWallet.hex_color).then(setWalletSvg)
      }
    } else {
      setWalletSvg("")
    }
  }, [isWalletDetailPage, currentWallet?.hex_color])

  // Determine if we should show the Add Wallet button in header
  // Show on main wallets page (if has wallets) and detail pages, but not on add wallet page
  const showAddWallet = !isAddWalletPage && (wallets.length > 0 || isWalletDetailPage) && !(isCloudMode && user?.is_admin) && !user?.is_demo

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <DemoBanner />
      <AppHeader
        showAddWallet={showAddWallet}
        customLogo={isWalletDetailPage ? walletSvg : undefined}
      />

      {/* Pass wallet data to children via React context */}
      <WalletsContext.Provider value={{ wallets, error, lastUpdate, isConnected, isLoading, currentWallet, setCurrentWallet, addWallet, refetchWallets }}>
        {children}
      </WalletsContext.Provider>

      <AppFooter />
    </div>
  )
}
