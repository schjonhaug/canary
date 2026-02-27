"use client"

import { useState, useEffect } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { Checkbox } from "@/components/ui/checkbox"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { ChevronDown, Loader2 } from "lucide-react"
import { useModal } from "@/hooks/useModal"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError } from "@/lib/utils"
import { ErrorDisplay } from "@/components/ui/error-display"
import { useAuth } from "@/contexts/auth-context"
import { Wallet } from "@/types"
import { XPUB_REGEX, DESCRIPTOR_REGEX, isValidBitcoinAddress, getDescriptorScriptType } from "@/lib/constants"
import { useTranslations } from "next-intl"

// Slug for the sample wallet route
export const SAMPLE_WALLET_SLUG = 'bacon'

// Well-known "bacon" test wallet (12x "bacon" as BIP39 mnemonic)
export const SAMPLE_WALLETS: Record<'mainnet' | 'testnet' | 'regtest', { name: string; descriptor: string }> = {
  mainnet: {
    name: "Bacon",
    descriptor: "wpkh([00000000/84h/0h/0h]xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K/<0;1>/*)#4jhrljfg",
  },
  testnet: {
    name: "Bacon",
    descriptor: "wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#4laqdwct",
  },
  regtest: {
    name: "Bacon",
    descriptor: "wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#4laqdwct",
  },
}

interface AddWalletFormProps {
  isFirstWallet?: boolean
  onWalletCreated: (wallet: Wallet) => void
  onCancel?: () => void
  autoFocusDescriptor?: boolean
  initialName?: string
  initialDescriptor?: string
  outputType?: 'descriptor' | 'xpub' | 'both'
}

export function AddWalletForm({
  isFirstWallet = false,
  onWalletCreated,
  onCancel,
  autoFocusDescriptor = false,
  initialName = "",
  initialDescriptor = "",
  outputType,
}: AddWalletFormProps) {
  const [name, setName] = useState(initialName)
  const [descriptor, setDescriptor] = useState(initialDescriptor)
  const [isFreshWallet, setIsFreshWallet] = useState(false)
  const [scriptType, setScriptType] = useState("")
  const [stopGap, setStopGap] = useState("")
  const [showAdvancedSettings, setShowAdvancedSettings] = useState(false)
  const modal = useModal()
  const { user } = useAuth()
  const t = useTranslations('wallets')
  const tApiErrors = useTranslations('errors.api')

  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_CANARY_MODE === 'cloud'

  // Sync state when initial values change (e.g., when blockHeader loads for Bacon wallet)
  useEffect(() => {
    if (initialName) {
      setName(initialName)
    }
  }, [initialName])

  useEffect(() => {
    if (initialDescriptor) {
      setDescriptor(initialDescriptor)
    }
  }, [initialDescriptor])

  // Prefill name on mount for first wallet in cloud mode (only if not already set)
  useEffect(() => {
    if (isFirstWallet && authEnabled && user?.name && !initialName) {
      setName(user.name)
    }
  }, [isFirstWallet, authEnabled, user?.name, initialName])

  // Set default script type for fresh XPUB wallets (auto not allowed)
  useEffect(() => {
    if (isXpubFormat(descriptor) && isFreshWallet && (!scriptType || scriptType === "auto")) {
      setScriptType("p2wpkh") // Default to Native SegWit (most common)
    }
  }, [descriptor, isFreshWallet, scriptType])

  // Helper function to detect Bitcoin address format
  const isAddressFormat = (input: string): boolean => {
    return isValidBitcoinAddress(input)
  }

  // Helper function to detect XPUB format (uses centralized pattern)
  const isXpubFormat = (input: string): boolean => {
    return XPUB_REGEX.test(input.trim())
  }

  // Helper function to detect output descriptor format (uses centralized pattern)
  const isDescriptorFormat = (input: string): boolean => {
    return DESCRIPTOR_REGEX.test(input.trim())
  }

  // Helper function to check if custom stop gap requires a specific script type
  // Custom stop gap with XPUB format requires explicit script type selection (not auto-detect)
  const needsScriptTypeForStopGap = (
    stopGapValue: string,
    descriptorValue: string,
    scriptTypeValue: string
  ): boolean => {
    const hasCustomStopGap = Boolean(stopGapValue) && stopGapValue !== "auto"
    const isXpub = !isDescriptorFormat(descriptorValue)
    const hasAutoScriptType = !scriptTypeValue || scriptTypeValue === "auto"
    return hasCustomStopGap && isXpub && hasAutoScriptType
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!name.trim()) {
      modal.setError(t('add.validation.nameRequired'))
      return
    }

    if (!descriptor.trim()) {
      modal.setError(t('add.validation.descriptorRequired'))
      return
    }

    // Skip advanced validation for Bitcoin addresses
    if (!isAddressFormat(descriptor)) {
      // Validate script type for fresh XPUB wallets
      if (isFreshWallet && isXpubFormat(descriptor) && !scriptType) {
        modal.setError(t('add.validation.scriptTypeRequired'))
        return
      }

      // Validate stop gap: custom stop gap requires specific script type (except for output descriptors)
      if (needsScriptTypeForStopGap(stopGap, descriptor, scriptType)) {
        modal.setError(t('add.stopGap.requiresScriptType'))
        return
      }
    }

    modal.setLoading(true)
    modal.clearError()

    try {
      // For Bitcoin addresses, skip script type and advanced settings
      const isAddress = isAddressFormat(descriptor)

      // Determine script type to send
      let finalScriptType: string | undefined

      if (!isAddress) {
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
      }

      const wallet = await api.createWallet({
        name: name.trim(),
        descriptor: descriptor.trim(),
        isFreshWallet: isAddress ? undefined : (isFreshWallet || undefined),
        scriptType: isAddress ? undefined : finalScriptType,
        stopGap: isAddress ? undefined : (stopGap || undefined),
      })
      onWalletCreated(wallet)
    } catch (err) {
      if (err instanceof ApiError) {
        modal.setError(getTranslatedApiError(err, tApiErrors))
      } else {
        modal.setError(err instanceof Error ? err.message : "Failed to add wallet")
      }
    } finally {
      modal.setLoading(false)
    }
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="wallet-name">{t('add.nameLabel')}</Label>
        <Input
          id="wallet-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          disabled={modal.isLoading}
          autoFocus={!autoFocusDescriptor}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="output-descriptor">
          {outputType === 'xpub'
            ? t('add.descriptorLabel.xpub')
            : outputType === 'descriptor'
              ? t('add.descriptorLabel.descriptor')
              : outputType === 'both'
                ? t('add.descriptorLabel.both')
                : t('add.descriptorLabel.any')}
        </Label>
        <Textarea
          id="output-descriptor"
          value={descriptor}
          onChange={(e) => setDescriptor(e.target.value)}
          disabled={modal.isLoading}
          rows={4}
          className="font-mono text-sm break-all whitespace-pre-wrap resize-none"
          autoFocus={autoFocusDescriptor}
        />
      </div>

      {/* Advanced Settings - only shown for XPUB or output descriptor input */}
      {(isXpubFormat(descriptor) || isDescriptorFormat(descriptor)) && <Collapsible open={showAdvancedSettings} onOpenChange={setShowAdvancedSettings}>
        <CollapsibleTrigger asChild>
          <Button
            variant="ghost"
            type="button"
            className="flex items-center justify-between w-full p-0 h-auto font-normal"
            disabled={modal.isLoading}
          >
            <span className="text-sm font-medium">{t('add.advancedSettings')}</span>
            <ChevronDown className={`h-4 w-4 transition-transform duration-200 ${showAdvancedSettings ? 'rotate-180' : ''}`} />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-4 pt-4">
          {/* Fresh wallet checkbox */}
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
              {t('add.freshWallet.label')}
            </Label>
          </div>

          {/* Script Type */}
          <div className="space-y-2">
            <Label htmlFor="script-type">{t('add.scriptType.label')}</Label>
            <Select
              value={isDescriptorFormat(descriptor) ? getDescriptorScriptType(descriptor) : (scriptType || (isFreshWallet && isXpubFormat(descriptor) ? "" : "auto"))}
              onValueChange={(value) => setScriptType(value)}
              disabled={modal.isLoading || isDescriptorFormat(descriptor)}
            >
              <SelectTrigger>
                <SelectValue placeholder={isFreshWallet && isXpubFormat(descriptor) ? t('add.scriptType.selectPlaceholder') : t('add.scriptType.autoPlaceholder')} />
              </SelectTrigger>
              <SelectContent>
                {!(isFreshWallet && isXpubFormat(descriptor)) && (
                  <SelectItem value="auto">{t('add.scriptType.auto')}</SelectItem>
                )}
                <SelectItem value="p2wpkh">{isFreshWallet && isXpubFormat(descriptor) ? t('add.scriptType.p2wpkhFresh') : t('add.scriptType.p2wpkh')}</SelectItem>
                <SelectItem value="p2sh">{isFreshWallet && isXpubFormat(descriptor) ? t('add.scriptType.p2shFresh') : t('add.scriptType.p2sh')}</SelectItem>
                <SelectItem value="p2pkh">{isFreshWallet && isXpubFormat(descriptor) ? t('add.scriptType.p2pkhFresh') : t('add.scriptType.p2pkh')}</SelectItem>
                <SelectItem value="p2tr">{isFreshWallet && isXpubFormat(descriptor) ? t('add.scriptType.p2trFresh') : t('add.scriptType.p2tr')}</SelectItem>
              </SelectContent>
            </Select>
            {isDescriptorFormat(descriptor) && (
              <p className="text-xs text-muted-foreground">
                {t('add.scriptType.detectedHint')}
              </p>
            )}
            {isFreshWallet && isXpubFormat(descriptor) && (
              <p className="text-xs text-muted-foreground">
                {t('add.scriptType.requiredHint')}
              </p>
            )}
          </div>

          {/* Stop Gap */}
          <div className="space-y-2">
            <Label htmlFor="stop-gap">{t('add.stopGap.label')}</Label>
            <Select
              value={stopGap || "auto"}
              onValueChange={(value) => setStopGap(value)}
              disabled={modal.isLoading}
            >
              <SelectTrigger>
                <SelectValue placeholder={t('add.stopGap.default')} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">{t('add.stopGap.default')}</SelectItem>
                <SelectItem value="250">{t('add.stopGap.extended')}</SelectItem>
                <SelectItem value="500">{t('add.stopGap.deep')}</SelectItem>
                <SelectItem value="750">{t('add.stopGap.deeper')}</SelectItem>
                <SelectItem value="1000">{t('add.stopGap.maximum')}</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              {t('add.stopGap.hint')}
            </p>
            {needsScriptTypeForStopGap(stopGap, descriptor, scriptType) && (
              <p className="text-xs text-red-500">
                {t('add.stopGap.requiresScriptType')}
              </p>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>}

      {modal.error && (
        <ErrorDisplay message={modal.error} variant="inline" className="[&_*]:break-all" />
      )}

      <div className="flex gap-3 pt-2">
        {onCancel && (
          <Button
            type="button"
            variant="outline"
            onClick={onCancel}
            disabled={modal.isLoading}
            className="flex-1"
          >
            Cancel
          </Button>
        )}
        <Button
          type="submit"
          disabled={modal.isLoading}
          className={onCancel ? "flex-1" : "w-full"}
        >
          {modal.isLoading ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              {t('add.submitting')}
            </>
          ) : (
            t('add.submit')
          )}
        </Button>
      </div>
    </form>
  )
}
