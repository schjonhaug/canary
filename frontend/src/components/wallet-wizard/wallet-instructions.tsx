"use client"

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { AddWalletForm } from "@/components/add-wallet-form"
import { type WalletGuide } from "@/lib/wallet-guides"
import { Wallet } from "@/types"
import { WizardBreadcrumb, WizardStep } from "./wizard-breadcrumb"

interface WalletInstructionsProps {
  selectedWallet: WalletGuide
  step: WizardStep
  onNavigateToChoose: () => void
  isFirstWallet: boolean
  onWalletCreated: (wallet: Wallet) => void
  getGuideSteps: (walletId: string) => string[]
  t: (key: string, params?: Record<string, string>) => string
  tNav: (key: string) => string
}

export function WalletInstructions({
  selectedWallet,
  step,
  onNavigateToChoose,
  isFirstWallet,
  onWalletCreated,
  getGuideSteps,
  t,
  tNav,
}: WalletInstructionsProps) {
  return (
    <div className="space-y-6">
      <WizardBreadcrumb
        selectedWallet={selectedWallet}
        step={step}
        onNavigateToChoose={onNavigateToChoose}
        t={t}
        tNav={tNav}
      />

      <div className="max-w-3xl mx-auto space-y-6">
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">{t('add.wizard.exportSteps', { walletName: selectedWallet.name })}</CardTitle>
          </CardHeader>
          <CardContent>
            <ol className="space-y-3">
              {getGuideSteps(selectedWallet.id).map((stepText, index) => (
                <li key={index} className="flex gap-3">
                  <span className="shrink-0 w-7 h-7 rounded-full bg-primary text-primary-foreground text-sm font-medium flex items-center justify-center">
                    {index + 1}
                  </span>
                  <span className="pt-0.5">{stepText}</span>
                </li>
              ))}
            </ol>
          </CardContent>
        </Card>

        {/* Form section */}
        <div className="text-center space-y-2">
          <p className="text-muted-foreground">
            {selectedWallet.outputType === 'xpub' ? t('add.wizard.pasteXpub') : t('add.wizard.pasteDescriptor')}
          </p>
        </div>

        <Card>
          <CardContent className="pt-6">
            <AddWalletForm
              isFirstWallet={isFirstWallet}
              onWalletCreated={onWalletCreated}
              autoFocusDescriptor={false}
            />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
