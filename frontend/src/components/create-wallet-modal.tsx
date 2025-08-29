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
import { Checkbox } from "@/components/ui/checkbox"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { ChevronDown } from "lucide-react"
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
  const [isFreshWallet, setIsFreshWallet] = useState(false)
  const [scriptType, setScriptType] = useState("")
  const [stopGap, setStopGap] = useState("")
  const [showAdvancedSettings, setShowAdvancedSettings] = useState(false)
  const modal = useModal()
  const { user } = useAuth()
  const descriptorRef = useRef<HTMLTextAreaElement>(null)
  const nameRef = useRef<HTMLInputElement>(null)
  
  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_CANARY_MODE === 'saas'
  
  // Prefill name when modal opens
  useEffect(() => {
    if (isOpen && isFirstWallet && authEnabled && user?.name) {
      setName(user.name)
    }
  }, [isOpen, isFirstWallet, authEnabled, user?.name])
  
  // Set default script type for fresh XPUB wallets
  useEffect(() => {
    if (isXpubFormat(descriptor) && isFreshWallet && !scriptType) {
      setScriptType("p2wpkh") // Default to Native SegWit (most common)
    }
  }, [descriptor, isFreshWallet, scriptType])
  
  // Helper function to detect XPUB format
  const isXpubFormat = (input: string): boolean => {
    const xpubRegex = /^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$/
    return xpubRegex.test(input.trim())
  }

  // Helper function to detect output descriptor format
  const isDescriptorFormat = (input: string): boolean => {
    const descriptorRegex = /^(wpkh|wsh|sh|pkh|tr)\(/
    return descriptorRegex.test(input.trim())
  }

  // Helper function to extract script type from descriptor
  const getDescriptorScriptType = (input: string): string => {
    const trimmed = input.trim()
    if (trimmed.startsWith('wpkh(')) return 'p2wpkh'
    if (trimmed.startsWith('wsh(')) return 'p2wsh'
    if (trimmed.startsWith('sh(wpkh(')) return 'p2sh'
    if (trimmed.startsWith('pkh(')) return 'p2pkh'
    if (trimmed.startsWith('tr(')) return 'p2tr'
    return ''
  }

  // Determine which field should be auto-focused
  const shouldFocusDescriptor = isOpen && isFirstWallet && authEnabled && user?.name
  const shouldFocusName = isOpen && !shouldFocusDescriptor

  const handleClose = () => {
    if (!modal.isLoading) {
      setName("")
      setDescriptor("")
      setIsFreshWallet(false)
      setScriptType("")
      setStopGap("")
      setShowAdvancedSettings(false)
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

    // Validate script type for fresh XPUB wallets
    if (isFreshWallet && isXpubFormat(descriptor) && !scriptType) {
      modal.setError("Script type is required for fresh XPUB wallets")
      return
    }

    // Validate stop gap: custom stop gap requires specific script type (except for output descriptors)
    if (stopGap && stopGap !== "auto") {
      // Skip script type requirement for output descriptors (they already contain script type info)
      if (!isDescriptorFormat(descriptor)) {
        if (!scriptType || scriptType === "auto") {
          modal.setError("Custom stop gap requires selecting a specific script type (not auto)")
          return
        }
      }
    }

    modal.setLoading(true)
    modal.clearError()

    try {
      // Determine script type to send
      let finalScriptType: string | undefined
      
      if (isFreshWallet && isXpubFormat(descriptor)) {
        // Fresh XPUB: always send script type
        finalScriptType = scriptType
      } else if (!isFreshWallet && isXpubFormat(descriptor) && scriptType && scriptType !== "auto") {
        // Existing XPUB with manually selected script type
        finalScriptType = scriptType
      } else if (isDescriptorFormat(descriptor)) {
        // Descriptor: extract script type for display purposes but don't send "auto"
        const extractedType = getDescriptorScriptType(descriptor)
        finalScriptType = extractedType || undefined
      }

      const wallet = await api.createWallet({
        name: name.trim(),
        descriptor: descriptor.trim(),
        isFreshWallet: isFreshWallet || undefined,
        scriptType: finalScriptType,
        stopGap: stopGap || undefined,
      })
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
          </div>

          {/* Fresh wallet checkbox - always show */}
          <div className="flex items-center space-x-2">
            <Checkbox
              id="fresh-wallet"
              checked={isFreshWallet}
              onCheckedChange={(checked) => setIsFreshWallet(checked === true)}
              disabled={modal.isLoading}
            />
            <Label
              htmlFor="fresh-wallet"
              className="text-sm font-normal cursor-pointer"
            >
              This is a fresh wallet (no transaction history)
            </Label>
          </div>

          {/* Script type dropdown - only show for fresh XPUB wallets */}
          {isFreshWallet && isXpubFormat(descriptor) && (
            <div className="space-y-2">
              <Label htmlFor="script-type">Address Type</Label>
              <Select value={scriptType} onValueChange={setScriptType} disabled={modal.isLoading}>
                <SelectTrigger>
                  <SelectValue placeholder="Select address type" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="p2wpkh">Native SegWit (bc1...) - Most common</SelectItem>
                  <SelectItem value="p2sh">Nested SegWit (3...) - Legacy compatibility</SelectItem>
                  <SelectItem value="p2tr">Taproot (bc1p...) - Modern</SelectItem>
                  <SelectItem value="p2pkh">Legacy (1...) - Oldest</SelectItem>
                </SelectContent>
              </Select>
            </div>
          )}

          {/* Advanced Settings */}
          <Collapsible open={showAdvancedSettings} onOpenChange={setShowAdvancedSettings}>
            <CollapsibleTrigger asChild>
              <Button
                variant="ghost" 
                type="button"
                className="flex items-center justify-between w-full p-0 h-auto font-normal"
                disabled={modal.isLoading}
              >
                <span className="text-sm font-medium">Advanced settings</span>
                <ChevronDown className={`h-4 w-4 transition-transform duration-200 ${showAdvancedSettings ? 'rotate-180' : ''}`} />
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="space-y-4 pt-4">
              {/* Script Type for advanced mode */}
              <div className="space-y-2">
                <Label htmlFor="advanced-script-type">Script Type</Label>
                <Select 
                  value={isDescriptorFormat(descriptor) ? getDescriptorScriptType(descriptor) : (scriptType || "auto")} 
                  onValueChange={(value) => setScriptType(value)}
                  disabled={modal.isLoading || isDescriptorFormat(descriptor)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Auto-detect" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="auto">Auto-detect (recommended)</SelectItem>
                    <SelectItem value="p2wpkh">Native SegWit (bc1...)</SelectItem>
                    <SelectItem value="p2sh">Nested SegWit (3...)</SelectItem>
                    <SelectItem value="p2pkh">Legacy (1...)</SelectItem>
                    <SelectItem value="p2tr">Taproot (bc1p...)</SelectItem>
                  </SelectContent>
                </Select>
                {isDescriptorFormat(descriptor) && (
                  <p className="text-xs text-muted-foreground">
                    Script type detected from descriptor and cannot be changed
                  </p>
                )}
              </div>

              {/* Stop Gap */}
              <div className="space-y-2">
                <Label htmlFor="stop-gap">Stop Gap</Label>
                <Select 
                  value={stopGap || "auto"} 
                  onValueChange={(value) => setStopGap(value)}
                  disabled={modal.isLoading}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Default (20 consecutive unused)" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="auto">Default (20 consecutive unused)</SelectItem>
                    <SelectItem value="250">Extended (250 consecutive unused)</SelectItem>
                    <SelectItem value="500">Deep (500 consecutive unused)</SelectItem>
                    <SelectItem value="750">Deeper (750 consecutive unused)</SelectItem>
                    <SelectItem value="1000">Maximum (1000 consecutive unused)</SelectItem>
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  Number of consecutive unused addresses to check before stopping. Increase if your wallet has addresses used at random high indices (e.g., BTCPay Server)
                </p>
                {stopGap && stopGap !== "auto" && !isDescriptorFormat(descriptor) && (!scriptType || scriptType === "auto") && (
                  <p className="text-xs text-red-500">
                    Custom stop gap requires selecting a specific script type
                  </p>
                )}
              </div>
            </CollapsibleContent>
          </Collapsible>

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