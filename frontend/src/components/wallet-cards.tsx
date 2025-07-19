"use client"

import { useEffect, useState, useMemo, memo } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Edit, Users, Filter } from "lucide-react"
import { DeleteWalletModal } from "./delete-wallet-modal"
import { EditWalletModal } from "./edit-wallet-modal"
import { loadCanarySvg, getCachedCanarySvg, formatBitcoinAmount, formatDate } from "@/lib/utils"

// Helper function to extract checksum from descriptor
function extractChecksum(descriptor: string): string {
  const checksumMatch = descriptor.match(/#([a-zA-Z0-9]+)$/)
  return checksumMatch ? checksumMatch[1] : "Unknown"
}
import { api } from "@/lib/api"
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

  // Component for loading SVG with synchronous cache to prevent flickering
  const WalletIcon = memo(({ wallet }: { wallet: Wallet }) => {
    // Try to get cached SVG first (synchronous)
    const cachedSvg = getCachedCanarySvg(wallet.hex_color)
    const [svgContent, setSvgContent] = useState<string>(cachedSvg || '')
    
    useEffect(() => {
      // If we already have cached content, don't reload
      if (cachedSvg) {
        return
      }
      
      let isMounted = true
      
      loadCanarySvg(wallet.hex_color).then(content => {
        if (isMounted) {
          setSvgContent(content)
        }
      })
      
      return () => { isMounted = false }
    }, [wallet.hex_color, cachedSvg])
    
    const checksumTitle = useMemo(() => 
      `Checksum: #${extractChecksum(wallet.descriptor)}`, 
      [wallet.descriptor]
    )
    
    return (
      <div 
        className="w-6 h-6 cursor-help flex-shrink-0"
        title={checksumTitle}
        dangerouslySetInnerHTML={{ __html: svgContent }}
      />
    )
  })

  const handleFilterClick = (walletId: number) => {
    if (selectedWalletId === walletId) {
      onSelectWallet(null) // Deselect if already selected
    } else {
      onSelectWallet(walletId)
    }
  }

  const handleEditClick = (wallet: Wallet) => {
    setWalletToEdit(wallet)
    setIsEditModalOpen(true)
  }

  const handleDeleteConfirm = async (walletId: number) => {
    await api.deleteWallet(walletId)
    
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
        {[...wallets].sort((a, b) => a.name.localeCompare(b.name)).map((wallet) => {
          const isSelected = selectedWalletId === wallet.id
          return (
            <Card 
              key={wallet.id} 
              className={`transition-all duration-200 ${
                isSelected 
                  ? "ring-2 ring-accent bg-accent/5 shadow-lg" 
                  : "hover:shadow-md hover:bg-muted/50"
              }`}
            >
              <CardHeader className="pb-3 relative">
                <div className="flex items-center gap-2">
                  <WalletIcon wallet={wallet} />
                  <CardTitle className="text-lg truncate pr-20" title={wallet.name}>
                    {wallet.name}
                  </CardTitle>
                </div>
                <div className="absolute top-2 right-2 flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    className={`h-8 w-8 p-0 transition-colors ${
                      isSelected 
                        ? "bg-accent/20 text-accent hover:bg-accent/30" 
                        : "hover:bg-accent/10 hover:text-accent"
                    }`}
                    onClick={() => handleFilterClick(wallet.id)}
                    title={isSelected ? "Remove filter" : "Filter transactions"}
                  >
                    <Filter className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-8 w-8 p-0 hover:bg-accent/10 hover:text-accent"
                    onClick={() => handleEditClick(wallet)}
                    title="Edit wallet"
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
                </div>
                <CardDescription className="text-xs text-muted-foreground">
                  {isSelected ? 'Filtering transactions below' : 'Use filter icon to view transactions'}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  <div>
                    <div className="text-sm text-muted-foreground">Balance</div>
                    <div className={`text-xl font-bold font-mono ${
                      isSelected ? "text-accent" : ""
                    }`}>
                      {formatBitcoinAmount(wallet.balance_total)}
                    </div>
                  </div>
                  <div className="flex justify-between items-center text-xs text-muted-foreground">
                    <span>
                      {wallet.last_activity 
                        ? `Last activity: ${formatDate(wallet.last_activity)}` 
                        : "No recent activity"
                      }
                    </span>
                    <div className="flex items-center gap-1">
                      <Users className="h-3 w-3" />
                      <span>{wallet.contact_count}</span>
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
        onDeleteWallet={handleDeleteFromEdit}
      />
    </div>
  )
}