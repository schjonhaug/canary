"use client"

import { useState, lazy, Suspense } from "react"
import { usePathname } from "next/navigation"
import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { useWalletsList } from "@/hooks/useWalletsList"

// Lazy load modal components for code splitting
const CreateWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))

export default function WalletsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const pathname = usePathname()
  
  // Only fetch wallets list on the main wallets page, not on detail pages
  const shouldFetchWallets = pathname === '/wallets'
  const { wallets, refresh: refetchWallets } = useWalletsList(shouldFetchWallets)

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
        showCreateWallet={wallets.length > 0}
        onCreateWallet={handleCreateWallet}
      />

      {children}

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