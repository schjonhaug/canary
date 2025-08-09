"use client"

import { useState, useEffect } from "react"
import { useParams, useRouter } from "next/navigation"
import { TransactionEvents } from "@/components/transaction-events"
import { InlineWalletNameEdit } from "@/components/inline-wallet-name-edit"
import { WalletContactsList } from "@/components/wallet-contacts-list"
import { ContactModal } from "@/components/contact-modal"
import { DeleteWalletModal } from "@/components/delete-wallet-modal"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { ArrowLeft, Trash2, AlertCircle, Plus } from "lucide-react"
import Link from "next/link"
import { useWalletDetail } from "@/hooks/useWalletDetail"
import { useWalletsContext } from "@/contexts/wallets-context"
import { formatBitcoinAmount } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { api } from "@/lib/api"

// Note: checksum is now available directly from wallet.checksum and URL params


export default function WalletDetailPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isAddContactModalOpen, setIsAddContactModalOpen] = useState(false)

  // Get wallet detail data directly using checksum
  const { wallet, events, contacts, error, isLoading, isConnected, lastUpdate, refresh } = useWalletDetail(checksum)
  
  // Share wallet data with layout context for SVG loading
  const { setCurrentWallet } = useWalletsContext()
  
  // Update context when wallet data changes
  useEffect(() => {
    if (setCurrentWallet) {
      setCurrentWallet(wallet)
    }
  }, [wallet, setCurrentWallet])
  

  const handleWalletUpdated = () => {
    refresh()
  }

  const handleNameUpdated = (newName: string) => {
    // Immediately update local state for responsive UI 
    if (wallet) {
      wallet.name = newName
    }
    handleWalletUpdated()
  }

  const handleDeleteWallet = async (checksum: string) => {
    await api.deleteWallet(checksum)
    router.push('/wallets')
  }


  if (isLoading && !wallet) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading wallet...</p>
        </div>
      </div>
    )
  }

  // Only show error if we have no cached data
  if (error && !wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              Back to Wallets
            </Button>
          </Link>
        </div>
        
        <Alert variant="destructive">
          <AlertTitle>Error</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      </>
    )
  }

  if (!wallet) {
    return (
      <>
        <div className="mb-6">
          <Link href="/wallets">
            <Button variant="ghost" size="sm" className="gap-2">
              <ArrowLeft size={16} />
              Back to Wallets
            </Button>
          </Link>
        </div>
        
        <Alert>
          <AlertTitle>Wallet Not Found</AlertTitle>
          <AlertDescription>
            The wallet with checksum #{checksum} could not be found.
          </AlertDescription>
        </Alert>
      </>
    )
  }

  return (
    <>
      {/* Connection Warning Banner */}
      {(!isConnected || (error && wallet)) && (
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
      
      <div className="space-y-8">
        {/* Header Section */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3 flex-wrap">
              <div>
                <nav className="flex items-center text-2xl text-muted-foreground">
                  <Link href="/wallets" className="hover:text-foreground font-semibold">
                    Wallets
                  </Link>
                  <span className="mx-2">/</span>
                  <div className="text-foreground font-semibold">
                    <InlineWalletNameEdit 
                      walletChecksum={wallet.checksum}
                      currentName={wallet.name}
                      onNameUpdated={handleNameUpdated}
                    />
                  </div>
                </nav>
              </div>
            </div>
          </div>
        </section>

        {/* Main Content */}
        <section>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Wallet Info Sidebar */}
        <div className="lg:col-span-1">
          <Card>
            <CardContent className="space-y-4">
              <div>
                <div className="text-sm text-muted-foreground">Balance</div>
                <div className="text-2xl font-bold font-mono">
                  {formatBitcoinAmount(wallet.balance_total || 0)}
                </div>
              </div>
              

              <div className="pt-2 border-t">
                <div className="flex items-center justify-between mb-2">
                  <div className="text-sm text-muted-foreground">Contacts</div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => setIsAddContactModalOpen(true)}
                    className="h-8 gap-1"
                  >
                    <Plus size={14} />
                    Add Contact
                  </Button>
                </div>
                <WalletContactsList 
                  walletChecksum={wallet.checksum}
                  contacts={contacts}
                  onContactsUpdated={handleWalletUpdated}
                />
              </div>

              <div className="pt-4 border-t flex justify-end">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsDeleteModalOpen(true)}
                  className="text-muted-foreground hover:text-red-600"
                >
                  <Trash2 size={16} />
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Transaction Events */}
        <div className="lg:col-span-2">
          <TransactionEvents 
            events={events}
            isConnected={isConnected}
            error={error}
            lastUpdate={lastUpdate}
            walletsCount={1} // Single wallet context
          />
        </div>
      </div>
        </section>
      </div>

      <ContactModal
        isOpen={isAddContactModalOpen}
        onClose={() => setIsAddContactModalOpen(false)}
        walletChecksum={wallet.checksum}
        onContactSaved={() => {
          setIsAddContactModalOpen(false)
          handleWalletUpdated()
        }}
      />

      <DeleteWalletModal
        wallet={wallet}
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        onConfirmDelete={() => handleDeleteWallet(wallet.checksum)}
      />
    </>
  )
}