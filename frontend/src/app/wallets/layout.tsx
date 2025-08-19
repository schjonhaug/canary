"use client"

import { useState, useEffect, lazy, Suspense } from "react"
import { usePathname } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { useWalletsList } from "@/hooks/useWalletsList"
import { loadCanarySvg, getCachedCanarySvg, hasReachedWalletLimit } from "@/lib/utils"
import { Wallet } from "@/types"
import { WalletsContext } from "@/contexts/wallets-context"
import { useAuth } from "@/contexts/auth-context"

// Lazy load modal components for code splitting
const AddWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))
const PlansModal = lazy(() => import("@/components/plans-modal").then(mod => ({ default: mod.PlansModal })))

export default function WalletsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [isAddWalletOpen, setIsAddWalletOpen] = useState(false)
  const [isUpgradeModalOpen, setIsUpgradeModalOpen] = useState(false)
  const [walletSvg, setWalletSvg] = useState<string>("")
  const [currentWallet, setCurrentWallet] = useState<Wallet | null>(null)
  const pathname = usePathname()
  const { user, billingStatus, isSaasMode, isFossMode } = useAuth()
  
  // Check if we're on a wallet detail page
  const isWalletDetailPage = pathname.startsWith('/wallets/') && pathname !== '/wallets'
  
  // Only fetch wallets list on the main wallets page, not on detail pages
  const shouldFetchWallets = pathname === '/wallets'
  const { wallets, error, lastUpdate, isConnected, refresh: refetchWallets } = useWalletsList(shouldFetchWallets)
  
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

  const handleAddWallet = () => {
    // In FOSS mode, no limits - always allow adding wallets
    if (isFossMode) {
      setIsAddWalletOpen(true)
      return
    }
    
    // Check wallet limits before opening create modal - use billing status as authoritative source
    const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
    if (hasReachedWalletLimit(wallets.length, currentTier)) {
      setIsUpgradeModalOpen(true)
      return
    }
    
    setIsAddWalletOpen(true)
  }

  const handleWalletAdded = () => {
    setIsAddWalletOpen(false)
    refetchWallets()
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <AppHeader 
        showAddWallet={wallets.length > 0 || isWalletDetailPage}
        onAddWallet={handleAddWallet}
        customLogo={isWalletDetailPage ? walletSvg : undefined}
      />

      {/* Pass wallet data to children via React context or props */}
      <WalletsContext.Provider value={{ wallets, error, lastUpdate, isConnected, onAddWallet: handleAddWallet, currentWallet, setCurrentWallet }}>
        {children}
      </WalletsContext.Provider>

      <Suspense fallback={null}>
        <AddWalletModal
          isOpen={isAddWalletOpen}
          onClose={() => setIsAddWalletOpen(false)}
          onWalletCreated={handleWalletAdded}
          isFirstWallet={wallets.length === 0}
        />
      </Suspense>

      <Suspense fallback={null}>
        <PlansModal
          isOpen={isUpgradeModalOpen}
          onClose={() => setIsUpgradeModalOpen(false)}
          currentTier={billingStatus?.subscription_tier || user?.subscription_tier || 'personal'}
          currentWalletCount={wallets.length}
          limitType="wallets"
          billingStatus={billingStatus ? {
            subscription_status: billingStatus.subscription_status,
            stripe_customer_id: billingStatus.stripe_customer_id
          } : undefined}
        />
      </Suspense>

      <AppFooter />
    </div>
  )
}