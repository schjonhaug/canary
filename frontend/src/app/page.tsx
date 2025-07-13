"use client"

import { useState } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"
import { SettingsModal } from "@/components/settings-modal"
import { Button } from "@/components/ui/button"
import { Settings } from "lucide-react"
import Image from "next/image"
import { useDashboard } from "@/hooks/useDashboard"
import { useBlockHeaders } from "@/hooks/useBlockHeaders"

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)
  const [isSettingsOpen, setIsSettingsOpen] = useState(false)
  const { wallets, events, isConnected, error } = useDashboard()
  const { blockHeader, connected, reconnecting, error: blockError } = useBlockHeaders(
    process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000'
  )

  const truncateHash = (hash: string) => {
    return `${hash.slice(0, 8)}...${hash.slice(-8)}`
  }

  const getConnectionSymbol = () => {
    if (reconnecting) return '🟡' // Yellow for connecting/reconnecting
    if (!connected || blockError) return '🔴' // Red for disconnected
    return '🟢' // Green for connected
  }

  const getConnectionTooltip = () => {
    if (reconnecting) return 'Reconnecting...'
    if (!connected || blockError) return 'Disconnected'
    return 'Connected'
  }

  return (
    <div className="container mx-auto py-8">
      <div className="mb-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Image
            src="/images/kanari.svg"
            alt="Kanari Logo"
            width={48}
            height={48}
            className="h-12 w-12"
          />
          <div>
            <h1 className="text-3xl font-bold uppercase tracking-wide">KANARI</h1>
            <p className="text-muted-foreground">Bitcoin Wallet Alert System</p>
          </div>
        </div>
        <div className="flex items-center gap-6">
          {/* Blockchain Status */}
          {blockHeader && (
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-2">
                <span 
                  className="text-lg cursor-help" 
                  title={getConnectionTooltip()}
                >
                  {getConnectionSymbol()}
                </span>
                <span className="text-muted-foreground">Block:</span>
                <span className="font-mono font-medium">{blockHeader.height.toLocaleString()}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Hash:</span>
                <span className="font-mono text-xs">{truncateHash(blockHeader.hash)}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">Time:</span>
                <span className="font-mono text-xs">
                  {new Date(blockHeader.timestamp * 1000).toLocaleString()}
                </span>
              </div>
            </div>
          )}
          
          {!blockHeader && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <span 
                className="text-lg cursor-help" 
                title={getConnectionTooltip()}
              >
                {getConnectionSymbol()}
              </span>
              <span>Loading blockchain data...</span>
            </div>
          )}

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
            <h2 className="text-2xl font-semibold">Wallets</h2>
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
          />
        </section>
      </div>
      
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
    </div>
  )
}
