"use client"

import Image from "next/image"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Lightbulb } from "lucide-react"
import { walletGuides, type WalletGuide } from "@/lib/wallet-guides"
import { AddWalletForm } from "@/components/add-wallet-form"
import { Wallet } from "@/types"
import { WizardBreadcrumb, WizardStep } from "./wizard-breadcrumb"

interface WalletTypeSelectorProps {
  selectedWallet: WalletGuide | null
  step: WizardStep
  onNavigateToChoose: () => void
  isSelfHostedMode: boolean
  isFirstWallet: boolean
  onSelectWallet: (wallet: WalletGuide) => void
  onSelectSampleWallet: () => void
  onWalletCreated: (wallet: Wallet) => void
  t: (key: string, params?: Record<string, string>) => string
  tNav: (key: string) => string
}

export function WalletTypeSelector({
  selectedWallet,
  step,
  onNavigateToChoose,
  isSelfHostedMode,
  isFirstWallet,
  onSelectWallet,
  onSelectSampleWallet,
  onWalletCreated,
  t,
  tNav,
}: WalletTypeSelectorProps) {
  return (
    <div className="space-y-6">
      <WizardBreadcrumb
        selectedWallet={selectedWallet}
        step={step}
        onNavigateToChoose={onNavigateToChoose}
        t={t}
        tNav={tNav}
      />

      <div className="max-w-3xl mx-auto space-y-10">
        {/* Form section */}
        <div className="text-center space-y-2">
          <p className="text-muted-foreground text-sm">
            {t('add.wizard.keysStaySafe')}
          </p>
        </div>

        {/* Bacon sample wallet for self-hosted first wallet */}
        {isSelfHostedMode && isFirstWallet && (
          <div className="flex items-center gap-3 p-4 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded-lg">
            <Lightbulb className="h-5 w-5 text-amber-600 dark:text-amber-500 shrink-0" />
            <p className="text-sm text-amber-700 dark:text-amber-300 flex-1">
              {t('add.wizard.tryBaconWallet')}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={onSelectSampleWallet}
              className="shrink-0 border-amber-300 dark:border-amber-700 text-amber-700 dark:text-amber-300 hover:bg-amber-100 dark:hover:bg-amber-900/50"
            >
              {t('add.wizard.useBaconWallet')}
            </Button>
          </div>
        )}

        <Card>
          <CardContent className="pt-6">
            <AddWalletForm
              isFirstWallet={isFirstWallet}
              onWalletCreated={onWalletCreated}
            />
          </CardContent>
        </Card>

        {/* Guides section */}
        <div className="space-y-6">
          <div className="text-center pt-4 border-t">
            <p className="text-muted-foreground text-sm">
              {t('add.wizard.guidesPrompt')}
            </p>
          </div>

          {/* Wallet grid */}
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
            {walletGuides.map((wallet) => (
              <button
                key={wallet.id}
                onClick={() => onSelectWallet(wallet)}
                className="flex flex-col items-center gap-3 p-6 rounded-xl border hover:bg-accent/5 hover:border-accent transition-all text-center group"
              >
                <div className="w-16 h-16 flex items-center justify-center">
                  <Image
                    src={wallet.logoSmall}
                    alt={wallet.name}
                    width={48}
                    height={48}
                    className="object-contain"
                  />
                </div>
                <div>
                  <div className="font-medium">{wallet.name}</div>
                  <div className="text-xs text-muted-foreground capitalize">{t(`add.wizard.walletType.${wallet.type}`)}</div>
                </div>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
