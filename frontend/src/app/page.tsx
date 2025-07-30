"use client"

import { useState, useEffect, lazy, Suspense } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"
import { Button } from "@/components/ui/button"
import { CircleCheckBig, LoaderCircle, CircleOff, Plus } from "lucide-react"
import Image from "next/image"
import { useDashboard } from "@/hooks/useDashboard"
import { useBlockHeaders } from "@/hooks/useBlockHeaders"
import { formatBitcoinAmount } from "@/lib/utils"
import { formatDistanceToNow } from 'date-fns'

// Lazy load modal components for code splitting
const CreateWalletModal = lazy(() => import("@/components/create-wallet-modal").then(mod => ({ default: mod.CreateWalletModal })))

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const [currentTime, setCurrentTime] = useState(Date.now())
  const { wallets, events, isConnected, error, lastUpdate } = useDashboard()
  const { blockHeader, connected, reconnecting, error: blockError } = useBlockHeaders()

  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentTime(Date.now());
    }, 60000); // Update every minute

    return () => clearInterval(interval);
  }, []);

  const formatTimeAgo = (timestamp: number) => {
    // Force re-render every minute by including currentTime in calculation
    currentTime; // This ensures the component re-renders when currentTime updates
    return formatDistanceToNow(new Date(timestamp * 1000), { addSuffix: true });
  };


  const getConnectionSymbol = () => {
    if (reconnecting) return <LoaderCircle size={16} className="text-yellow-500 animate-spin" />
    if (!connected || blockError) return <CircleOff size={16} className="text-red-500" />
    return <CircleCheckBig size={16} className="text-green-500" />
  }

  const getConnectionTooltip = () => {
    if (reconnecting) return 'Reconnecting...'
    if (!connected || blockError) return 'Disconnected'
    return 'Connected'
  }

  const handleCreateWallet = () => {
    setIsCreateWalletOpen(true)
  }

  const handleWalletCreated = () => {
    setIsCreateWalletOpen(false)
  }

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + wallet.balance_total
    }, 0)
  }

  return (
    <div className="container mx-auto py-8">
      <div className="mb-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={48}
            height={48}
            className="h-12 w-12"
          />
          <h1 className="text-3xl font-bold uppercase tracking-wide">CANARY</h1>
        </div>
        <div className="flex items-center gap-6">
          {/* Blockchain Status */}
          {blockHeader && (
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-2">
                <div 
                  className="cursor-help" 
                  title={getConnectionTooltip()}
                >
                  {getConnectionSymbol()}
                </div>
                <span className="text-muted-foreground">Block height:</span>
                <span className="font-mono font-medium">{blockHeader.height.toLocaleString()}</span>
              </div>
              <div className="flex items-center gap-2 hidden lg:flex">
                <span className="text-muted-foreground">Time:</span>
                <span>
                  {formatTimeAgo(blockHeader.timestamp)}
                </span>
              </div>
            </div>
          )}
          
          {!blockHeader && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <div 
                className="cursor-help" 
                title={getConnectionTooltip()}
              >
                {getConnectionSymbol()}
              </div>
              <span>Loading blockchain data...</span>
            </div>
          )}

          <Button
            onClick={handleCreateWallet}
            size="sm"
            className="bg-accent hover:bg-accent/90 text-accent-foreground gap-2"
          >
            <Plus size={16} />
            Create Wallet
          </Button>
          
        </div>
      </div>
      
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
          />
        </section>
      </div>
      
      
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
              <h3 className="text-lg font-bold uppercase tracking-wide">CANARY</h3>
              <p className="text-muted-foreground text-sm">Bitcoin Wallet Alert System</p>
            </div>
          </div>
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
