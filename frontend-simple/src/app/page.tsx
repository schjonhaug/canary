import { TransactionEvents } from "@/components/transaction-events"
import { WalletCards } from "@/components/wallet-cards"

export default function Home() {
  return (
    <div className="container mx-auto py-8">
      <div className="mb-6">
        <h1 className="text-3xl font-bold">TxRay</h1>
        <p className="text-muted-foreground">Bitcoin Wallet Management System</p>
      </div>
      
      <div className="space-y-8">
        {/* Wallet Cards Section */}
        <section>
          <h2 className="text-2xl font-semibold mb-4">Wallets</h2>
          <WalletCards />
        </section>

        {/* Transaction Events Section */}
        <section>
          <h2 className="text-2xl font-semibold mb-4">Transaction Events</h2>
          <TransactionEvents />
        </section>
      </div>
    </div>
  )
}
