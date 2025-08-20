"use client"

import { useState, useEffect, useRef } from "react"
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
import { useAuth } from "@/contexts/auth-context"
import { Wallet } from "@/types"

interface CreateWalletModalProps {
  isOpen: boolean
  onClose: () => void
  onWalletCreated: (wallet?: Wallet) => void
  isFirstWallet?: boolean
}


export function CreateWalletModal({
  isOpen,
  onClose,
  onWalletCreated,
  isFirstWallet = false,
}: CreateWalletModalProps) {
  const [name, setName] = useState("")
  const [descriptor, setDescriptor] = useState("")
  const modal = useModal()
  const { user } = useAuth()
  const descriptorRef = useRef<HTMLTextAreaElement>(null)
  const nameRef = useRef<HTMLInputElement>(null)
  
  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED === 'true'
  
  // Prefill name when modal opens
  useEffect(() => {
    if (isOpen && isFirstWallet && authEnabled && user?.name) {
      setName(user.name)
    }
  }, [isOpen, isFirstWallet, authEnabled, user?.name])
  
  // Determine which field should be auto-focused
  const shouldFocusDescriptor = isOpen && isFirstWallet && authEnabled && user?.name
  const shouldFocusName = isOpen && !shouldFocusDescriptor

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
      modal.setError("Output descriptor or extended public key is required")
      return
    }

    modal.setLoading(true)
    modal.clearError()

    try {
      const wallet = await api.createWallet(name.trim(), descriptor.trim())
      onWalletCreated(wallet)
      handleClose()
    } catch (err) {
      modal.setError(err instanceof Error ? err.message : "Failed to add wallet")
    } finally {
      modal.setLoading(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Add Wallet for Monitoring
          </DialogTitle>
          <DialogDescription>
            Add an existing wallet for monitoring by providing a name and output descriptor or extended public key (XPUB).
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="wallet-name">Wallet Name</Label>
            <Input
              ref={nameRef}
              id="wallet-name"
              type="text"
              placeholder="Enter wallet name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={modal.isLoading}
              autoFocus={shouldFocusName}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="output-descriptor">Output Descriptor or Extended Public Key</Label>
            <Textarea
              ref={descriptorRef}
              id="output-descriptor"
              placeholder="Enter output descriptor (wpkh(xpub.../<0;1>/*)) or extended public key (xpub/ypub/zpub...)"
              value={descriptor}
              onChange={(e) => setDescriptor(e.target.value)}
              disabled={modal.isLoading}
              rows={4}
              className="font-mono text-sm break-all whitespace-pre-wrap resize-none"
              autoFocus={!!shouldFocusDescriptor}
            />
            <div className="text-xs text-muted-foreground space-y-1">
              <p>• <strong>Output descriptor</strong>: wpkh([fingerprint/path]xpub.../&#60;0;1&#62;/*)</p>
              <p>• <strong>Extended public key</strong>: xpub/ypub/zpub... (will auto-detect script type)</p>
            </div>
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
                  Adding...
                </>
              ) : (
                "Add Wallet"
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}