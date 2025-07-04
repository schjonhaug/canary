"use client"

import { useState } from "react"
import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"

export default function Home() {
  const [selectedWalletId, setSelectedWalletId] = useState<number | null>(null)

  return (
    <div className="container mx-auto py-8">
      <div className="mb-6">
        <h1 className="text-3xl font-bold">TxRay</h1>
        <p className="text-muted-foreground">Bitcoin Wallet Management System</p>
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
    </div>
  )
}
