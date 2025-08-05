"use client"

import { useState, useEffect, lazy, Suspense, createContext, useContext } from "react"
import { usePathname, useParams } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { useWalletsList } from "@/hooks/useWalletsList"
import { useWalletDetail } from "@/hooks/useWalletDetail"
import { loadCanarySvg, getCachedCanarySvg } from "@/lib/utils"
import { Wallet } from "@/types"

// Create context for sharing wallet data
interface WalletsContextType {
  wallets: Wallet[]
  error: string | null
  lastUpdate: number | null
  isConnected: boolean
  onCreateWallet: () => void
}

const WalletsContext = createContext<WalletsContextType | null>(null)

export const useWalletsContext = () => {
  const context = useContext(WalletsContext)
  if (!context) {
    throw new Error('useWalletsContext must be used within WalletsLayout')
  }
  return context
}

// Lazy load modal components for code splitting
const CreateWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))

export default function WalletsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const [walletSvg, setWalletSvg] = useState<string>("")
  const pathname = usePathname()
  const params = useParams()
  
  // Check if we're on a wallet detail page
  const isWalletDetailPage = pathname.startsWith('/wallets/') && pathname !== '/wallets'
  const checksum = isWalletDetailPage ? params.checksum as string : null
  
  // Only fetch wallets list on the main wallets page, not on detail pages
  const shouldFetchWallets = pathname === '/wallets'
  const { wallets, error, lastUpdate, isConnected, refresh: refetchWallets } = useWalletsList(shouldFetchWallets)
  
  // Get wallet detail data for custom logo if on detail page
  const { wallet } = useWalletDetail(checksum)
  
  // Load SVG when wallet data is available for detail pages
  useEffect(() => {
    if (isWalletDetailPage && wallet?.hex_color) {
      const cachedSvg = getCachedCanarySvg(wallet.hex_color)
      if (cachedSvg) {
        setWalletSvg(cachedSvg)
      } else {
        loadCanarySvg(wallet.hex_color).then(setWalletSvg)
      }
    } else {
      setWalletSvg("")
    }
  }, [isWalletDetailPage, wallet?.hex_color])

  const handleCreateWallet = () => {
    setIsCreateWalletOpen(true)
  }

  const handleWalletCreated = () => {
    setIsCreateWalletOpen(false)
    refetchWallets()
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <AppHeader 
        showCreateWallet={wallets.length > 0 || isWalletDetailPage}
        onCreateWallet={handleCreateWallet}
        customLogo={isWalletDetailPage ? walletSvg : undefined}
      />

      {/* Pass wallet data to children via React context or props */}
      <WalletsContext.Provider value={{ wallets, error, lastUpdate, isConnected, onCreateWallet: handleCreateWallet }}>
        {children}
      </WalletsContext.Provider>

      <Suspense fallback={null}>
        <CreateWalletModal
          isOpen={isCreateWalletOpen}
          onClose={() => setIsCreateWalletOpen(false)}
          onWalletCreated={handleWalletCreated}
        />
      </Suspense>

      <AppFooter />
    </div>
  )
}