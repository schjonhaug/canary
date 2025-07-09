"use client"

import { useState } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"
import { SettingsModal } from "@/components/settings-modal"
// ContactsModal removed - contacts are now wallet-specific
import { BlockStatus } from "@/components/block-status"
import { Button } from "@/components/ui/button"
import { Settings } from "lucide-react"

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)
  const [isSettingsOpen, setIsSettingsOpen] = useState(false)
  // isContactsOpen removed - contacts are now wallet-specific

  return (
    <div className="container mx-auto py-8">
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Kanari</h1>
          <p className="text-muted-foreground">Bitcoin Wallet Management System</p>
        </div>
        <div className="flex gap-2">
          {/* Global Contacts button removed - contacts are now wallet-specific */}
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
      
      {/* Block Status Section */}
      <div className="mb-6">
        <BlockStatus />
      </div>
      
      <div className="space-y-8">
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
          <TransactionEvents selectedWalletId={selectedWalletId} />
        </section>
      </div>
      
      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
      
      {/* ContactsModal removed - contacts are now wallet-specific */}
    </div>
  )
}
