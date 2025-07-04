import { TransactionEvents } from "@/components/transaction-events"

export default function Home() {
  return (
    <div className="container mx-auto py-8">
      <div className="mb-6">
        <h1 className="text-3xl font-bold">TxRay</h1>
        <p className="text-muted-foreground">Bitcoin Wallet Management System</p>
      </div>
      <TransactionEvents />
    </div>
  )
}
