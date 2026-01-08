"use client"

import Link from "next/link"
import { WalletGuide } from "@/lib/wallet-guides"

export type WizardStep = 'choose' | 'instructions' | 'form'

interface WizardBreadcrumbProps {
  selectedWallet: WalletGuide | null
  step: WizardStep
  onNavigateToChoose: () => void
  t: (key: string, params?: Record<string, string>) => string
  tNav: (key: string) => string
}

export function WizardBreadcrumb({
  selectedWallet,
  step,
  onNavigateToChoose,
  t,
  tNav,
}: WizardBreadcrumbProps) {
  return (
    <nav className="flex items-center text-2xl text-muted-foreground flex-wrap">
      <Link href="/wallets" className="hover:text-foreground font-semibold">
        {tNav('wallets')}
      </Link>
      <span className="mx-2">/</span>
      {step === 'choose' ? (
        <span className="text-foreground font-semibold">{tNav('addWallet')}</span>
      ) : (
        <button
          onClick={onNavigateToChoose}
          className="hover:text-foreground font-semibold"
        >
          {tNav('addWallet')}
        </button>
      )}
      {step === 'instructions' && selectedWallet && (
        <>
          <span className="mx-2">/</span>
          <span className="text-foreground font-semibold">{selectedWallet.name}</span>
        </>
      )}
      {step === 'form' && (
        <>
          <span className="mx-2">/</span>
          <span className="text-foreground font-semibold">{t('add.wizard.enterDetails')}</span>
        </>
      )}
    </nav>
  )
}
