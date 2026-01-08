"use client"

import { useState, useEffect, use } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import { Skeleton } from "@/components/ui/skeleton"
import { useAuth } from "@/contexts/auth-context"
import { useWalletsContext } from "@/contexts/wallets-context"
import { api } from "@/lib/api"
import { hasReachedWalletLimit } from "@/lib/utils"
import { useBlockHeader } from "@/hooks/useBlockHeader"
import { useWalletWizard } from "@/hooks/useWalletWizard"
import { Wallet } from "@/types"
import { useTranslations } from "next-intl"
import { WizardBreadcrumb } from "@/components/wallet-wizard/wizard-breadcrumb"
import { WalletTypeSelector } from "@/components/wallet-wizard/wallet-type-selector"
import { WalletInstructions } from "@/components/wallet-wizard/wallet-instructions"
import { WalletFormStep } from "@/components/wallet-wizard/wallet-form-step"
import { UpgradePrompt } from "@/components/upgrade-prompt"

interface PageProps {
  params: Promise<{ slug?: string[] }>
}

function AddWalletPageContent({ slug }: { slug?: string[] }) {
  const router = useRouter()
  const { user, billingStatus, isSelfHostedMode, isCloudMode, isLoading: authLoading, isAuthenticated, refreshBillingStatus } = useAuth()
  const { wallets, isLoading: isLoadingWallets, addWallet } = useWalletsContext()
  const { blockHeader } = useBlockHeader()
  const [isUpgrading, setIsUpgrading] = useState(false)
  const [upgradingTier, setUpgradingTier] = useState<string | null>(null)
  const t = useTranslations('wallets')
  const tNav = useTranslations('nav')

  // Get network for Bacon wallet
  const network = blockHeader?.network ?? 'mainnet'

  // Use wallet wizard hook for step management
  const {
    step,
    selectedWallet,
    isBaconWallet,
    baconWallet,
    handleNavigateToChoose,
    handleSelectWallet,
    handleSkipToForm,
    handleSelectSampleWallet,
    getGuideSteps,
  } = useWalletWizard({ slug, network, t: t as unknown as { raw: (key: string) => unknown } })

  // Derive wallet count and limit status from context
  const walletCount = wallets.length

  // Redirect to sign-in if not authenticated in cloud mode
  useEffect(() => {
    if (!authLoading && isCloudMode && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [authLoading, isCloudMode, isAuthenticated, router])

  // Derive limit reached status from context data
  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
  const limitReached = isCloudMode && !isSelfHostedMode && hasReachedWalletLimit(walletCount, currentTier)

  const handleWalletCreated = async (wallet: Wallet) => {
    addWallet?.(wallet)
    await refreshBillingStatus()
    router.push('/wallets')
  }

  const handleUpgrade = async (targetTier: string, isYearly: boolean = false) => {
    if (!isAuthenticated) {
      window.location.href = '/sign-up'
      return
    }

    try {
      setIsUpgrading(true)
      setUpgradingTier(targetTier)

      const { url } = await api.createCheckoutSession(targetTier, isYearly)

      setTimeout(() => {
        refreshBillingStatus()
      }, 1000)

      window.location.href = url
    } catch (error) {
      console.error('Failed to create checkout session:', error)
      alert(t('add.checkoutError'))
    } finally {
      setIsUpgrading(false)
      setUpgradingTier(null)
    }
  }

  const isFirstWallet = walletCount === 0
  const hasPaidSubscription = billingStatus?.subscription_status === 'active' && !!billingStatus?.stripe_customer_id

  // Loading state
  if (authLoading || isLoadingWallets) {
    return (
      <div className="space-y-6">
        <nav className="flex items-center text-2xl text-muted-foreground">
          <Link href="/wallets" className="hover:text-foreground font-semibold">
            {tNav('wallets')}
          </Link>
          <span className="mx-2">/</span>
          <Skeleton className="h-8 w-32" />
        </nav>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-4 max-w-2xl mx-auto">
          {[...Array(6)].map((_, i) => (
            <Skeleton key={i} className="h-28" />
          ))}
        </div>
      </div>
    )
  }

  // Upgrade prompt if limit reached
  if (limitReached && isCloudMode) {
    return (
      <div className="space-y-6">
        <WizardBreadcrumb
          selectedWallet={null}
          step="choose"
          onNavigateToChoose={handleNavigateToChoose}
          t={t}
          tNav={tNav}
        />
        <UpgradePrompt
          limitType="wallets"
          currentTier={currentTier}
          onUpgrade={handleUpgrade}
          isLoading={isUpgrading}
          loadingTier={upgradingTier}
          hasPaidSubscription={hasPaidSubscription}
        />
      </div>
    )
  }

  // Step 1: Choose your wallet
  if (step === 'choose') {
    return (
      <WalletTypeSelector
        selectedWallet={selectedWallet}
        step={step}
        onNavigateToChoose={handleNavigateToChoose}
        isSelfHostedMode={isSelfHostedMode}
        isFirstWallet={isFirstWallet}
        onSelectWallet={handleSelectWallet}
        onSkipToForm={handleSkipToForm}
        onSelectSampleWallet={handleSelectSampleWallet}
        t={t}
        tNav={tNav}
      />
    )
  }

  // Step 2: Show instructions for selected wallet (with form included)
  if (step === 'instructions' && selectedWallet) {
    return (
      <WalletInstructions
        selectedWallet={selectedWallet}
        step={step}
        onNavigateToChoose={handleNavigateToChoose}
        isFirstWallet={isFirstWallet}
        onWalletCreated={handleWalletCreated}
        getGuideSteps={getGuideSteps}
        t={t}
        tNav={tNav}
      />
    )
  }

  // Step 3: Form (either directly or after instructions)
  return (
    <WalletFormStep
      selectedWallet={selectedWallet}
      step={step}
      onNavigateToChoose={handleNavigateToChoose}
      isBaconWallet={isBaconWallet}
      isFirstWallet={isFirstWallet}
      baconWallet={baconWallet}
      onWalletCreated={handleWalletCreated}
      t={t}
      tNav={tNav}
    />
  )
}

export default function AddWalletPage({ params }: PageProps) {
  const { slug } = use(params)
  return <AddWalletPageContent slug={slug} />
}
