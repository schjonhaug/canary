"use client"

import { useState } from "react"
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
import { useModal } from "@/hooks/useModal"
import { api } from "@/lib/api"
import { ErrorDisplay } from "@/components/ui/error-display"
import { Loader2 } from "lucide-react"

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
  const modal = useModal()

  const handleClose = () => {
    if (!modal.isLoading) {
      setName("")
      setDescriptor("")
      modal.reset()
      onClose()
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!name.trim()) {
      modal.setError("Wallet name is required")
      return
    }
    
    if (!descriptor.trim()) {
      modal.setError("Output descriptor is required")
      return
    }

    modal.setLoading(true)
    modal.clearError()

    try {
      await api.createWallet(name.trim(), descriptor.trim())
      onWalletCreated()
      handleClose()
    } catch (err) {
      modal.setError(err instanceof Error ? err.message : "Failed to create wallet")
    } finally {
      modal.setLoading(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
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
              disabled={modal.isLoading}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="output-descriptor">Output Descriptor</Label>
            <Textarea
              id="output-descriptor"
              placeholder="Enter multipath output descriptor (e.g., wpkh([fingerprint/derivation_path]xpub.../0/*)"
              value={descriptor}
              onChange={(e) => setDescriptor(e.target.value)}
              disabled={modal.isLoading}
              rows={4}
              className="font-mono text-sm break-all whitespace-pre-wrap resize-none"
            />
            <p className="text-xs text-muted-foreground">
              Must be a valid multipath output descriptor with checksum
            </p>
          </div>

          {modal.error && (
            <ErrorDisplay message={modal.error} variant="inline" />
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={handleClose}
              disabled={modal.isLoading}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={modal.isLoading}
            >
              {modal.isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Creating...
                </>
              ) : (
                "Create Wallet"
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}