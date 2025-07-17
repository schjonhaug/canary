"use client"

import { useState } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"
import { SettingsModal } from "@/components/settings-modal"
import { CreateWalletModal } from "@/components/create-wallet-modal"
import { Button } from "@/components/ui/button"
import { Settings, CircleCheckBig, LoaderCircle, CircleOff, Plus } from "lucide-react"
import Image from "next/image"
import { useDashboard } from "@/hooks/useDashboard"
import { useBlockHeaders } from "@/hooks/useBlockHeaders"
import { formatBitcoinAmount } from "@/lib/utils"

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)
  const [isSettingsOpen, setIsSettingsOpen] = useState(false)
  const [isCreateWalletOpen, setIsCreateWalletOpen] = useState(false)
  const { wallets, events, isConnected, error, lastUpdate, isUsingCache } = useDashboard()
  const { blockHeader, connected, reconnecting, error: blockError } = useBlockHeaders(
    process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000'
  )

  const truncateHash = (hash: string) => {
    return `${hash.slice(0, 8)}...${hash.slice(-8)}`
  }

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
                <span className="text-muted-foreground">Block:</span>
                <span className="font-mono font-medium">{blockHeader.height.toLocaleString()}</span>
              </div>
              <div className="flex items-center gap-2 hidden md:flex">
                <span className="text-muted-foreground">Hash:</span>
                <span className="font-mono text-xs">{truncateHash(blockHeader.hash)}</span>
              </div>
              <div className="flex items-center gap-2 hidden lg:flex">
                <span className="text-muted-foreground">Time:</span>
                <span className="font-mono text-xs">
                  {new Date(blockHeader.timestamp * 1000).toLocaleString()}
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
          
          <Button
            variant="outline"
            size="sm"
            onClick={() => setIsSettingsOpen(true)}
            className="gap-2"
          >
            <Settings size={16} />
            Settings
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
              {isUsingCache && (
                <span className="text-xs bg-chart-2/20 text-chart-2 border border-chart-2/30 px-2 py-1 rounded-full">
                  Cached Data
                </span>
              )}
            </div>
            {selectedWalletId && (
              <button
                onClick={() => setSelectedWalletId(null)}
                className="text-sm text-muted-foreground hover:text-foreground underline"
              >
                Clear selection
              </button>
            )}
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
      
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
      
      <CreateWalletModal
        isOpen={isCreateWalletOpen}
        onClose={() => setIsCreateWalletOpen(false)}
        onWalletCreated={handleWalletCreated}
      />
      
      {/* Footer */}
      <footer className="mt-16 pt-8 border-t border-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Image
              src="/images/kanari.svg"
              alt="Kanari Logo"
              width={48}
              height={48}
              className="h-12 w-12"
            />
            <div>
              <h3 className="text-lg font-bold uppercase tracking-wide">KANARI</h3>
              <p className="text-muted-foreground text-sm">Bitcoin Wallet Alert System</p>
            </div>
          </div>
          <a 
            href="https://github.com/schjonhaug/kanari" 
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
