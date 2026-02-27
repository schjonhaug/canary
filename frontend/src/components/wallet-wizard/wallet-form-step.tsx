"use client"

import { Card, CardContent } from "@/components/ui/card"
import { AddWalletForm } from "@/components/add-wallet-form"
import { type WalletGuide } from "@/lib/wallet-guides"
import { Wallet } from "@/types"
import { WizardBreadcrumb, WizardStep } from "./wizard-breadcrumb"

interface WalletFormStepProps {
  selectedWallet: WalletGuide | null
  step: WizardStep
  onNavigateToChoose: () => void
  isBaconWallet: boolean
  isFirstWallet: boolean
  baconWallet: { name: string; descriptor: string }
  onWalletCreated: (wallet: Wallet) => void
  t: (key: string, params?: Record<string, string>) => string
  tNav: (key: string) => string
}

export function WalletFormStep({
  selectedWallet,
  step,
  onNavigateToChoose,
  isBaconWallet,
  isFirstWallet,
  baconWallet,
  onWalletCreated,
  t,
  tNav,
}: WalletFormStepProps) {
  return (
    <div className="space-y-6">
      <WizardBreadcrumb
        selectedWallet={selectedWallet}
        step={step}
        onNavigateToChoose={onNavigateToChoose}
        t={t}
        tNav={tNav}
      />

      <div className="max-w-xl mx-auto space-y-6">
        <div className="text-center space-y-2">
          <p className="text-muted-foreground">
            {isBaconWallet
              ? t('add.wizard.baconPrefilled')
              : (selectedWallet?.outputType === 'xpub' ? t('add.wizard.pasteXpub') : t('add.wizard.pasteDescriptor'))
            }
          </p>
        </div>

        <Card>
          <CardContent className="pt-6">
            <AddWalletForm
              isFirstWallet={isFirstWallet}
              onWalletCreated={onWalletCreated}
              autoFocusDescriptor={false}
              initialName={isBaconWallet ? baconWallet.name : undefined}
              initialDescriptor={isBaconWallet ? baconWallet.descriptor : undefined}
              outputType={selectedWallet?.outputType}
            />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
