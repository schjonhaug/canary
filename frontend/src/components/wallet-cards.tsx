"use client"

import { useEffect, useState } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Edit, Users } from "lucide-react"
import { DeleteWalletModal } from "./delete-wallet-modal"
import { EditWalletModal } from "./edit-wallet-modal"
import { extractChecksum, loadCanarySvg } from "@/lib/utils"
import { Wallet } from "../types"

interface WalletCardsProps {
  selectedWalletId: number | null
  onSelectWallet: (walletId: number | null) => void
  wallets: Wallet[]
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
}

export function WalletCards({ selectedWalletId, onSelectWallet, wallets, error, lastUpdate }: WalletCardsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [walletToDelete, setWalletToDelete] = useState<Wallet | null>(null)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const [walletToEdit, setWalletToEdit] = useState<Wallet | null>(null)
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  // Component for loading SVG asynchronously
  const WalletIcon = ({ checksum }: { checksum: string }) => {
    const [svgContent, setSvgContent] = useState<string>('')
    
    useEffect(() => {
      loadCanarySvg(checksum).then(setSvgContent)
    }, [checksum])
    
    return (
      <div 
        className="w-6 h-6 cursor-help flex-shrink-0"
        title={`Checksum: #${checksum}`}
        dangerouslySetInnerHTML={{ __html: svgContent }}
      />
    )
  }

  const formatBalance = (sats: number | null) => {
    if (sats === null) return "0.00000000 BTC"
    const btc = sats / 100_000_000
    return `${btc.toLocaleString(undefined, { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
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

    // Wallet will be removed from state automatically via SSE
    
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


  const handleWalletUpdated = () => {
    // Wallet list will be updated automatically via SSE
    setIsEditModalOpen(false)
  }

  if (!hasReceivedData) {
    return (
      <div className="space-y-4">
        {/* Individual Wallet Card Skeletons */}
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <Card key={i} className="hover:shadow-md hover:bg-gray-50/50">
              <CardHeader className="pb-3 relative">
                <div className="flex items-center justify-between">
                  <Skeleton className="h-6 w-32" />
                  <div className="flex items-center gap-2">
                    <Skeleton className="h-8 w-8 rounded-md" />
                    <Skeleton className="h-6 w-16" />
                  </div>
                </div>
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <Skeleton className="h-4 w-16 mb-1" />
                    <Skeleton className="h-6 w-40" />
                  </div>
                  <div className="flex justify-between items-center">
                    <Skeleton className="h-3 w-32" />
                    <div className="flex items-center gap-1">
                      <Skeleton className="h-3 w-3" />
                      <Skeleton className="h-3 w-4" />
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
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
              No wallets found. Use the &quot;Create Wallet&quot; button in the header to get started.
            </CardDescription>
          </CardHeader>
        </Card>

      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Individual Wallet Cards */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {wallets.map((wallet) => {
          const isSelected = selectedWalletId === wallet.id
          return (
            <Card 
              key={wallet.id} 
              className={`cursor-pointer transition-all duration-200 ${
                isSelected 
                  ? "ring-2 ring-accent bg-accent/5 shadow-lg" 
                  : "hover:shadow-md hover:bg-muted/50"
              }`}
              onClick={() => handleWalletClick(wallet.id)}
            >
              <CardHeader className="pb-3 relative">
                <div className="flex items-center gap-2">
                  <WalletIcon checksum={extractChecksum(wallet.descriptor)} />
                  <CardTitle className="text-lg truncate pr-16" title={wallet.name}>
                    {wallet.name}
                  </CardTitle>
                </div>
                <div className="absolute top-2 right-2 flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-8 w-8 p-0 hover:bg-accent/10 hover:text-accent"
                    onClick={(e) => handleEditClick(wallet, e)}
                    title="Edit wallet"
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
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
                      isSelected ? "text-accent" : ""
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