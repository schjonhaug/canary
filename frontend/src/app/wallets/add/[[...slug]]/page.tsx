"use client"

import { useState, useEffect, useMemo, use } from "react"
import { useRouter } from "next/navigation"
import Link from "next/link"
import Image from "next/image"
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { AlertTriangle, ChevronRight, Lightbulb } from "lucide-react"
import { useAuth } from "@/contexts/auth-context"
import { useWalletsContext } from "@/contexts/wallets-context"
import { api } from "@/lib/api"
import { hasReachedWalletLimit, getTierDisplayName, getWalletLimit } from "@/lib/utils"
import { AddWalletForm, SAMPLE_WALLETS, SAMPLE_WALLET_SLUG } from "@/components/add-wallet-form"
import { PlanComparison } from "@/components/plan-comparison"
import { walletGuides, type WalletGuide } from "@/lib/wallet-guides"
import { useBlockHeader } from "@/hooks/useBlockHeader"
import { Wallet } from "@/types"

type WizardStep = 'choose' | 'instructions' | 'form'

// Props for shared breadcrumb navigation
interface BreadcrumbProps {
  selectedWallet: WalletGuide | null
  step: WizardStep
  onNavigateToChoose: () => void
}

// Props for the Choose step
interface ChooseStepProps {
  breadcrumbProps: BreadcrumbProps
  isSelfHostedMode: boolean
  isFirstWallet: boolean
  onSelectWallet: (wallet: WalletGuide) => void
  onSkipToForm: () => void
  onSelectSampleWallet: () => void
}

// Props for the Instructions step (now includes form)
interface InstructionsStepProps {
  breadcrumbProps: BreadcrumbProps
  selectedWallet: WalletGuide
  isFirstWallet: boolean
  onWalletCreated: (wallet: Wallet) => void
}

// Props for the Form step
interface FormStepProps {
  breadcrumbProps: BreadcrumbProps
  selectedWallet: WalletGuide | null
  isBaconWallet: boolean
  isFirstWallet: boolean
  baconWallet: { name: string; descriptor: string }
  onWalletCreated: (wallet: Wallet) => void
}

// Breadcrumb component
function Breadcrumb({
  selectedWallet,
  step,
  onNavigateToChoose,
}: {
  selectedWallet: WalletGuide | null
  step: WizardStep
  onNavigateToChoose: () => void
}) {
  return (
    <nav className="flex items-center text-2xl text-muted-foreground flex-wrap">
      <Link href="/wallets" className="hover:text-foreground font-semibold">
        Wallets
      </Link>
      <span className="mx-2">/</span>
      {step === 'choose' ? (
        <span className="text-foreground font-semibold">Add Wallet</span>
      ) : (
        <button
          onClick={onNavigateToChoose}
          className="hover:text-foreground font-semibold"
        >
          Add Wallet
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
          <span className="text-foreground font-semibold">Enter Details</span>
        </>
      )}
    </nav>
  )
}

// Step 1: Choose your wallet
function ChooseStep({
  breadcrumbProps,
  isSelfHostedMode,
  isFirstWallet,
  onSelectWallet,
  onSkipToForm,
  onSelectSampleWallet,
}: ChooseStepProps) {
  return (
    <div className="space-y-6">
      <Breadcrumb {...breadcrumbProps} />

      <div className="max-w-3xl mx-auto space-y-8">
        <div className="text-center space-y-2">
          <p className="text-muted-foreground text-lg">
            Which wallet do you use? We&apos;ll show you how to export your output descriptor or XPUB.
          </p>
          <p className="text-muted-foreground text-sm">
            Canary only needs this to monitor your transactions. Your private keys stay safe in your wallet.
          </p>
        </div>

        {/* Bacon sample wallet for self-hosted first wallet */}
        {isSelfHostedMode && isFirstWallet && (
          <div className="flex items-center gap-3 p-4 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded-lg">
            <Lightbulb className="h-5 w-5 text-amber-600 dark:text-amber-500 shrink-0" />
            <p className="text-sm text-amber-700 dark:text-amber-300 flex-1">
              New here? Try with a sample wallet first to see how Canary works.
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={onSelectSampleWallet}
              className="shrink-0 border-amber-300 dark:border-amber-700 text-amber-700 dark:text-amber-300 hover:bg-amber-100 dark:hover:bg-amber-900/50"
            >
              Use Bacon Wallet
            </Button>
          </div>
        )}

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
                <div className="text-xs text-muted-foreground capitalize">{wallet.type}</div>
              </div>
            </button>
          ))}
        </div>

        {/* Skip to form option */}
        <div className="text-center pt-4 border-t">
          <button
            onClick={onSkipToForm}
            className="text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1 text-sm"
          >
            I already have my output descriptor or XPUB
            <ChevronRight size={16} />
          </button>
        </div>
      </div>
    </div>
  )
}

// Step 2: Show instructions for selected wallet with form below
function InstructionsStep({
  breadcrumbProps,
  selectedWallet,
  isFirstWallet,
  onWalletCreated,
}: InstructionsStepProps) {
  return (
    <div className="space-y-6">
      <Breadcrumb {...breadcrumbProps} />

      <div className="max-w-3xl mx-auto space-y-6">
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Follow these steps to export your descriptor from {selectedWallet.name}</CardTitle>
          </CardHeader>
          <CardContent>
            <ol className="space-y-3">
              {selectedWallet.steps.map((stepText, index) => (
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
            Paste your {selectedWallet.outputType === 'xpub' ? 'XPUB' : 'output descriptor or XPUB'} below.
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

// Step 3: Form (either directly or after instructions)
function FormStep({
  breadcrumbProps,
  selectedWallet,
  isBaconWallet,
  isFirstWallet,
  baconWallet,
  onWalletCreated,
}: FormStepProps) {
  return (
    <div className="space-y-6">
      <Breadcrumb {...breadcrumbProps} />

      <div className="max-w-xl mx-auto space-y-6">
        <div className="text-center space-y-2">
          <p className="text-muted-foreground">
            {isBaconWallet
              ? "We've prefilled the Bacon sample wallet for you. Just click Add Wallet to continue."
              : `Paste your ${selectedWallet?.outputType === 'xpub' ? 'XPUB' : 'output descriptor or XPUB'} below.`
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
            />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

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

  // Derive wallet count and limit status from context
  const walletCount = wallets.length

  // Get network for Bacon wallet
  const network = blockHeader?.network ?? 'mainnet'
  const baconWallet = SAMPLE_WALLETS[network]

  // Check if we're using the Bacon sample wallet
  const isBaconWallet = slug?.[0] === SAMPLE_WALLET_SLUG

  // Derive wizard state from URL path segments
  // /wallets/add → choose
  // /wallets/add/sparrow → instructions + form for sparrow
  // /wallets/add/form → form without wallet context (skipped instructions)
  // /wallets/add/bacon → form with Bacon wallet prefilled
  const selectedWallet = useMemo(() => {
    if (!slug || slug.length === 0) return null
    if (slug[0] === 'form' || slug[0] === SAMPLE_WALLET_SLUG) return null
    return walletGuides.find(w => w.id === slug[0]) || null
  }, [slug])

  const step: WizardStep = useMemo(() => {
    if (!slug || slug.length === 0) return 'choose'
    if (slug[0] === 'form' || slug[0] === SAMPLE_WALLET_SLUG) return 'form'
    if (selectedWallet) return 'instructions'
    return 'choose'
  }, [slug, selectedWallet])

  // Redirect invalid wallet IDs to clean choose URL
  // Also redirect legacy /wallets/add/{wallet-id}/form URLs to /wallets/add/{wallet-id}
  useEffect(() => {
    if (slug && slug.length > 0 && slug[0] !== 'form' && slug[0] !== SAMPLE_WALLET_SLUG && !selectedWallet) {
      router.replace('/wallets/add')
    }
    // Redirect legacy /wallets/add/{wallet-id}/form to /wallets/add/{wallet-id}
    if (slug && slug.length >= 2 && slug[1] === 'form' && selectedWallet) {
      router.replace(`/wallets/add/${selectedWallet.id}`)
    }
  }, [slug, selectedWallet, router])

  // Redirect to sign-in if not authenticated in cloud mode
  useEffect(() => {
    if (!authLoading && isCloudMode && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [authLoading, isCloudMode, isAuthenticated, router])

  // Derive limit reached status from context data
  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
  const limitReached = isCloudMode && !isSelfHostedMode && hasReachedWalletLimit(walletCount, currentTier)

  const handleWalletCreated = (wallet: Wallet) => {
    // Add wallet to context immediately so it appears in the list
    addWallet?.(wallet)
    router.push('/wallets')
  }

  const handleNavigateToChoose = () => {
    router.push('/wallets/add')
  }

  const handleSelectWallet = (wallet: WalletGuide) => {
    router.push(`/wallets/add/${wallet.id}`)
  }

  const handleSkipToForm = () => {
    router.push('/wallets/add/form')
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
      alert('Failed to start checkout. Please try again.')
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
        {/* Breadcrumb skeleton */}
        <nav className="flex items-center text-2xl text-muted-foreground">
          <Link href="/wallets" className="hover:text-foreground font-semibold">
            Wallets
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
    const walletLimit = getWalletLimit(currentTier)
    return (
      <div className="space-y-6">
        <Breadcrumb
          selectedWallet={null}
          step="choose"
          onNavigateToChoose={handleNavigateToChoose}
        />

        <Card className="border-amber-200 bg-amber-50 dark:border-amber-900 dark:bg-amber-950/30">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-amber-800 dark:text-amber-400">
              <AlertTriangle className="h-5 w-5" />
              Wallet Limit Reached
            </CardTitle>
            <CardDescription className="text-amber-700 dark:text-amber-500">
              You&apos;ve reached your {getTierDisplayName(currentTier)} plan limit of {walletLimit} wallet{walletLimit !== 1 ? 's' : ''}.
              Upgrade to add more wallets.
            </CardDescription>
          </CardHeader>
        </Card>

        <PlanComparison
          currentTier={currentTier}
          onUpgrade={handleUpgrade}
          highlightUpgrades={true}
          showPricing={true}
          isModal={false}
          isLoading={isUpgrading}
          loadingTier={upgradingTier}
          hasPaidSubscription={hasPaidSubscription}
        />
      </div>
    )
  }

  // Shared breadcrumb props for all steps
  const breadcrumbProps: BreadcrumbProps = {
    selectedWallet,
    step,
    onNavigateToChoose: handleNavigateToChoose,
  }

  // Step 1: Choose your wallet
  if (step === 'choose') {
    return (
      <ChooseStep
        breadcrumbProps={breadcrumbProps}
        isSelfHostedMode={isSelfHostedMode}
        isFirstWallet={isFirstWallet}
        onSelectWallet={handleSelectWallet}
        onSkipToForm={handleSkipToForm}
        onSelectSampleWallet={() => router.push(`/wallets/add/${SAMPLE_WALLET_SLUG}`)}
      />
    )
  }

  // Step 2: Show instructions for selected wallet (with form included)
  if (step === 'instructions' && selectedWallet) {
    return (
      <InstructionsStep
        breadcrumbProps={breadcrumbProps}
        selectedWallet={selectedWallet}
        isFirstWallet={isFirstWallet}
        onWalletCreated={handleWalletCreated}
      />
    )
  }

  // Step 3: Form (either directly or after instructions)
  return (
    <FormStep
      breadcrumbProps={breadcrumbProps}
      selectedWallet={selectedWallet}
      isBaconWallet={isBaconWallet}
      isFirstWallet={isFirstWallet}
      baconWallet={baconWallet}
      onWalletCreated={handleWalletCreated}
    />
  )
}

export default function AddWalletPage({ params }: PageProps) {
  const { slug } = use(params)
  return <AddWalletPageContent slug={slug} />
}
