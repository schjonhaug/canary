"use client"

import { useState } from "react"
import { Plus } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"

interface CreateWalletModalProps {
  isOpen: boolean
  onClose: () => void
  onWalletCreated: () => void
}

export function CreateWalletModal({
  isOpen,
  onClose,
  onWalletCreated,
}: CreateWalletModalProps) {
  const [name, setName] = useState("")
  const [descriptor, setDescriptor] = useState("")
  const [isCreating, setIsCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleClose = () => {
    if (!isCreating) {
      setName("")
      setDescriptor("")
      setError(null)
      onClose()
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!name.trim()) {
      setError("Wallet name is required")
      return
    }
    
    if (!descriptor.trim()) {
      setError("Output descriptor is required")
      return
    }

    setIsCreating(true)
    setError(null)

    try {
      const baseUrl = process.env.NEXT_PUBLIC_API_URL!
      const apiUrl = `${baseUrl}/wallets`
      const response = await fetch(apiUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          name: name.trim(),
          descriptor: descriptor.trim(),
        }),
      })

      if (!response.ok) {
        if (response.status === 400) {
          const errorData = await response.json()
          throw new Error(errorData.error || "Invalid wallet data")
        }
        if (response.status === 409) {
          throw new Error("A wallet with this descriptor already exists")
        }
        throw new Error(`Failed to create wallet: ${response.status}`)
      }

      // Wallet created successfully
      onWalletCreated()
      handleClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create wallet")
    } finally {
      setIsCreating(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Plus className="h-5 w-5" />
            Create New Wallet
          </DialogTitle>
          <DialogDescription>
            Create a new wallet by providing a name and output descriptor.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="wallet-name">Wallet Name</Label>
            <Input
              id="wallet-name"
              type="text"
              placeholder="Enter wallet name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={isCreating}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="output-descriptor">Output Descriptor</Label>
            <Textarea
              id="output-descriptor"
              placeholder="Enter multipath output descriptor (e.g., wpkh([fingerprint/derivation_path]xpub.../0/*)"
              value={descriptor}
              onChange={(e) => setDescriptor(e.target.value)}
              disabled={isCreating}
              rows={4}
              className="font-mono text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Must be a valid multipath output descriptor with checksum
            </p>
          </div>

          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg">
              <p className="text-sm text-red-700">{error}</p>
            </div>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={handleClose}
              disabled={isCreating}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={isCreating}
            >
              {isCreating ? "Creating..." : "Create Wallet"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}