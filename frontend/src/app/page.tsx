"use client"

import { useState, lazy, Suspense } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"
import { Button } from "@/components/ui/button"
import { Plus, AlertCircle } from "lucide-react"
import Image from "next/image"
import { useDashboard } from "@/hooks/useDashboard"
import { formatBitcoinAmount } from "@/lib/utils"
import { useAuth } from "@/contexts/auth-context"
import { UserDropdown } from "@/components/user-dropdown"
import { WalletOnboarding } from "@/components/wallet-onboarding"
import { useRelativeTime } from "@/hooks/useRelativeTime"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

// Lazy load modal components for code splitting
const CreateWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const { wallets, events, blockHeader, error, lastUpdate, isConnected, refresh } = useDashboard()
  const { isAuthenticated, isLoading } = useAuth()
  const blockHeaderTime = useRelativeTime(blockHeader?.timestamp)


  const handleCreateWallet = () => {
    setIsCreateWalletOpen(true)
  }

  const handleWalletCreated = () => {
    setIsCreateWalletOpen(false)
    refresh() // Immediately refresh dashboard after wallet creation
  }

  const handleWalletDeleted = () => {
    refresh() // Immediately refresh dashboard after wallet deletion
  }

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + wallet.balance_total
    }, 0)
  }

  // Show landing page for logged out users
  if (!isLoading && !isAuthenticated) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={120}
            height={120}
            className="mx-auto mb-6"
          />
          <h1 className="text-4xl font-bold tracking-wide">Canary</h1>
        </div>
      </div>
    )
  }

  // Show loading state while auth is loading
  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // Show dashboard for authenticated users
  return (
    <div className="container mx-auto py-8">
      {/* Connection Warning Banner */}
      {!isConnected && (
        <Alert variant="destructive" className="mb-6">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>Backend Connection Lost</AlertTitle>
          <AlertDescription>
            Unable to connect to the backend service. Displaying cached data.
            {lastUpdate && (
              <span className="block mt-1 text-xs">
                Last updated: {new Date(lastUpdate * 1000).toLocaleString()}
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}
      <div className="mb-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={48}
            height={48}
            className="h-12 w-12"
          />
          <h1 className="text-3xl font-bold tracking-wide">Canary</h1>
        </div>
        <div className="flex items-center gap-6">
          {wallets.length > 0 && (
            <Button
              onClick={handleCreateWallet}
              size="sm"
              className="bg-accent hover:bg-accent/90 text-accent-foreground gap-2"
            >
              <Plus size={16} />
              Create Wallet
            </Button>
          )}
          
          <UserDropdown />
        </div>
      </div>
      
      {wallets.length === 0 ? (
        <WalletOnboarding onCreateWallet={handleCreateWallet} />
      ) : (
        <div className="mt-8 space-y-8">
          {/* Wallet Cards Section */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-3 flex-wrap">
                <div>
                  <h2 className="text-2xl font-semibold">Wallets</h2>
                  {wallets.length > 1 && (
                    <p className="text-sm text-muted-foreground">
                      Tracking {wallets.length} wallets with a total balance of {formatBitcoinAmount(getTotalBalance())}
                    </p>
                  )}
                </div>
              </div>
            </div>
            <WalletCards 
              selectedWalletId={selectedWalletId}
              onSelectWallet={setSelectedWalletId}
              wallets={wallets}
              isConnected={isConnected}
              error={error}
              lastUpdate={lastUpdate}
              onWalletDeleted={handleWalletDeleted}
            />
          </section>

          {/* Transaction Events Section */}
          <section>
            <h2 className="text-2xl font-semibold mb-4">
              Transaction Events
              {selectedWalletId && (
                <span className="text-lg text-muted-foreground ml-2">
                  (filtered)
                </span>
              )}
            </h2>
            <TransactionEvents 
              selectedWalletId={selectedWalletId} 
              events={events}
              isConnected={isConnected}
              error={error}
              lastUpdate={lastUpdate}
              walletsCount={wallets.length}
            />
          </section>
        </div>
      )}
      
      
      <Suspense fallback={null}>
        <CreateWalletModal
          isOpen={isCreateWalletOpen}
          onClose={() => setIsCreateWalletOpen(false)}
          onWalletCreated={handleWalletCreated}
        />
      </Suspense>
      
      {/* Footer */}
      <footer className="mt-16 pt-8 border-t border-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Image
              src="/images/canary-in-a-coalmine.svg"
              alt="Canary Logo"
              width={48}
              height={48}
              className="h-12 w-12"
            />
            <div>
              <h3 className="text-lg font-bold tracking-wide">Canary</h3>
              <p className="text-muted-foreground text-sm">Bitcoin Wallet Alert System</p>
            </div>
          </div>
          
          {/* Blockchain Info */}
          {blockHeader && (
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Block height:</span>
                <span className="font-mono font-medium">{blockHeader.height.toLocaleString()}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Time:</span>
                <span>{blockHeaderTime}</span>
              </div>
            </div>
          )}
          
          <a 
            href="https://github.com/schjonhaug/canary" 
            target="_blank" 
            rel="noopener noreferrer"
            className="text-muted-foreground hover:text-accent transition-colors flex items-center gap-2"
          >
            <Image
              src="/images/github.svg"
              alt="GitHub"
              width={20}
              height={20}
              className="h-5 w-5"
            />
            <span>GitHub</span>
          </a>
        </div>
      </footer>
    </div>
  )
}
