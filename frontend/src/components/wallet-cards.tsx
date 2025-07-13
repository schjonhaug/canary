"use client"

import { useEffect, useState } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Edit, Plus, Users } from "lucide-react"
import { DeleteWalletModal } from "./delete-wallet-modal"
import { CreateWalletModal } from "./create-wallet-modal"
import { EditWalletModal } from "./edit-wallet-modal"
import { extractChecksum } from "@/lib/utils"

interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  created_at: string
  balance_total?: number
  last_activity?: string
  contact_count?: number
}

interface WalletCardsProps {
  selectedWalletId: number | null
  onSelectWallet: (walletId: number | null) => void
  wallets: Wallet[]
  isConnected: boolean
  error: string | null
}

export function WalletCards({ selectedWalletId, onSelectWallet, wallets, isConnected, error }: WalletCardsProps) {
  const [loading, setLoading] = useState(true)
  const [walletToDelete, setWalletToDelete] = useState<Wallet | null>(null)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false)
  const [walletToEdit, setWalletToEdit] = useState<Wallet | null>(null)
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)

  // Set loading to false once we have data
  useEffect(() => {
    setLoading(false)
  }, [wallets])

  const formatBalance = (sats?: number) => {
    if (sats === undefined || sats === null) return "0.00000000 BTC"
    const btc = sats / 100_000_000
    return `${btc.toLocaleString(undefined, { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
  }

  const getTotalBalance = () => {
    return wallets.reduce((total, wallet) => {
      return total + (wallet.balance_total || 0)
    }, 0)
  }


  const handleWalletClick = (walletId: number) => {
    if (selectedWalletId === walletId) {
      onSelectWallet(null) // Deselect if already selected
    } else {
      onSelectWallet(walletId)
    }
  }

  const handleEditClick = (wallet: Wallet, event: React.MouseEvent) => {
    event.stopPropagation() // Prevent wallet selection when clicking edit
    setWalletToEdit(wallet)
    setIsEditModalOpen(true)
  }

  const handleDeleteConfirm = async (walletId: number) => {
    const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
    const response = await fetch(`${baseUrl}/api/wallets/${walletId}`, {
      method: 'DELETE',
    })

    if (!response.ok) {
      if (response.status === 404) {
        throw new Error('Wallet not found')
      }
      throw new Error(`Delete failed: ${response.status}`)
    }

    // Remove wallet from local state
    setWallets(prev => prev.filter(w => w.id !== walletId))
    
    // Clear selection if the deleted wallet was selected
    if (selectedWalletId === walletId) {
      onSelectWallet(null)
    }
  }

  const handleDeleteModalClose = () => {
    setIsDeleteModalOpen(false)
    setWalletToDelete(null)
  }

  const handleEditModalClose = () => {
    setIsEditModalOpen(false)
    setWalletToEdit(null)
  }

  const handleDeleteFromEdit = (wallet: Wallet) => {
    // Close edit modal and open delete modal
    setIsEditModalOpen(false)
    setWalletToEdit(null)
    setWalletToDelete(wallet)
    setIsDeleteModalOpen(true)
  }

  const handleCreateWallet = () => {
    setIsCreateModalOpen(true)
  }

  const handleCreateModalClose = () => {
    setIsCreateModalOpen(false)
  }

  const handleWalletCreated = () => {
    // Wallet list will be updated automatically via SSE
    setIsCreateModalOpen(false)
  }

  const handleWalletUpdated = () => {
    // Wallet list will be updated automatically via SSE
    setIsEditModalOpen(false)
  }

  if (loading) {
    return (
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle>Loading...</CardTitle>
            <CardDescription>Fetching wallet data...</CardDescription>
          </CardHeader>
        </Card>
      </div>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Error</CardTitle>
          <CardDescription className="text-destructive">
            Failed to load wallets: {error}
          </CardDescription>
        </CardHeader>
      </Card>
    )
  }

  if (wallets.length === 0) {
    return (
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <CardTitle>No Wallets</CardTitle>
            <CardDescription>
              No wallets found. Create your first wallet to get started.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Button
              onClick={handleCreateWallet}
              className="bg-orange-600 hover:bg-orange-700"
            >
              <Plus className="h-4 w-4 mr-2" />
              Create Your First Wallet
            </Button>
          </CardContent>
        </Card>

        <CreateWalletModal
          isOpen={isCreateModalOpen}
          onClose={handleCreateModalClose}
          onWalletCreated={handleWalletCreated}
        />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Total Balance Summary Card */}
      <Card className="bg-gradient-to-r from-orange-50 to-amber-50 border-orange-200">
        <CardHeader className="py-4">
          <CardTitle>
            <div className="flex items-center gap-4">
              <span>Total Balance</span>
              <div className="text-2xl font-bold font-mono text-orange-800">
                {formatBalance(getTotalBalance())}
              </div>
              <Badge variant="outline" className="text-xs">
                {wallets.length} wallet{wallets.length !== 1 ? 's' : ''}
              </Badge>
            </div>
          </CardTitle>
        </CardHeader>
      </Card>

      {/* Individual Wallet Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {wallets.map((wallet) => {
          const isSelected = selectedWalletId === wallet.id
          return (
            <Card 
              key={wallet.id} 
              className={`cursor-pointer transition-all duration-200 ${
                isSelected 
                  ? "ring-2 ring-orange-500 bg-orange-50/50 shadow-lg" 
                  : "hover:shadow-md hover:bg-gray-50/50"
              }`}
              onClick={() => handleWalletClick(wallet.id)}
            >
              <CardHeader className="pb-3 relative">
                <CardTitle className="text-lg truncate pr-20" title={wallet.name}>
                  {wallet.name}
                </CardTitle>
                <div className="absolute top-2 right-2 flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-8 w-8 p-0 hover:bg-blue-50 hover:text-blue-600"
                    onClick={(e) => handleEditClick(wallet, e)}
                    title="Edit wallet"
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
                </div>
                <div className="absolute top-2 right-12 text-xs font-mono text-muted-foreground bg-gray-100 px-2 py-1 rounded">
                  #{extractChecksum(wallet.descriptor)}
                </div>
                <CardDescription className="text-xs text-muted-foreground">
                  Click to {isSelected ? 'deselect' : 'view transactions'}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <div className="text-sm text-muted-foreground">Balance</div>
                    <div className={`text-xl font-bold font-mono ${
                      isSelected ? "text-orange-700" : ""
                    }`}>
                      {formatBalance(wallet.balance_total)}
                    </div>
                  </div>
                  <div className="flex justify-between items-center text-xs text-muted-foreground">
                    <span>
                      {wallet.last_activity 
                        ? `Last activity: ${new Date(wallet.last_activity).toLocaleDateString()}` 
                        : "No recent activity"
                      }
                    </span>
                    <div className="flex items-center gap-1">
                      <Users className="h-3 w-3" />
                      <span>{wallet.contact_count || 0}</span>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          )
        })}
      </div>

      <DeleteWalletModal
        wallet={walletToDelete}
        isOpen={isDeleteModalOpen}
        onClose={handleDeleteModalClose}
        onConfirmDelete={handleDeleteConfirm}
      />

      <CreateWalletModal
        isOpen={isCreateModalOpen}
        onClose={handleCreateModalClose}
        onWalletCreated={handleWalletCreated}
      />

      <EditWalletModal
        wallet={walletToEdit}
        isOpen={isEditModalOpen}
        onClose={handleEditModalClose}
        onWalletUpdated={handleWalletUpdated}
        onDeleteWallet={handleDeleteFromEdit}
      />
    </div>
  )
}