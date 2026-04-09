"use client"

import Image from "next/image"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { ErrorDisplay } from "@/components/ui/error-display"
import { CheckCircle2, Loader2, Github } from "lucide-react"
import { allFeatures } from "@/lib/pricing-data"
import { usePricing, formatPrice, sortTiers } from "@/hooks/usePricing"
import { useTranslations } from "next-intl"
import { useLocale } from "next-intl"

// Map tier slug to translation key (handles selfhosted -> selfHosted)
function getTierTranslationKey(tier: string): string {
  return tier === 'selfhosted' ? 'selfHosted' : tier
}

// Map feature ID to translation key
function getFeatureTranslationKey(featureId: string): string {
  const mapping: Record<string, string> = {
    'wallets': 'wallets',
    'contacts': 'contacts',
    'sync': 'sync',
    'trial': 'trial',
    'email': 'email',
    'sms': 'sms',
    'push': 'push',
    'balance-alerts': 'balanceAlerts',
    'analysis': 'analysis',
    'own-node': 'ownNode',
    'privacy': 'privacy',
    'subscription': 'noSubscription'
  }
  return mapping[featureId] || featureId
}

interface PlanComparisonProps {
  currentTier: string
  onUpgrade?: (targetTier: string, isYearly: boolean) => void
  highlightUpgrades?: boolean
  showPricing?: boolean
  isModal?: boolean
  showCallToAction?: boolean
  showUnifiedTrialButton?: boolean
  showAllTiers?: boolean
  isTrialUser?: boolean
  isLoading?: boolean
  loadingTier?: string | null
  hasPaidSubscription?: boolean
}

export function PlanComparison({
  currentTier,
  onUpgrade,
  highlightUpgrades = true,
  showPricing = true,
  isModal = false,
  showCallToAction = false,
  showUnifiedTrialButton = false,
  showAllTiers = false,
  isTrialUser = false,
  isLoading = false,
  loadingTier = null,
  hasPaidSubscription = false
}: PlanComparisonProps) {
  const { pricing, loading, error } = usePricing()
  const t = useTranslations('billing')

  // Add self-hosted tier manually (not from Stripe) - only for public landing page
  const selfHostedTier = {
    tier: 'selfhosted',
    name: 'Self-hosted',
    description: 'Run on your own infrastructure',
    features: {}
  }

  // Only use Stripe pricing; append self-hosted only on public pages (not in modal)
  // Paid tiers (Personal, Team) come first since those are where we make money
  const stripeTiers = pricing ? sortTiers(pricing.tiers) : []
  const sortedTiers = isModal ? stripeTiers : [...stripeTiers, selfHostedTier]
  
  // Filter tiers to show only current tier and higher tiers for modal (unless showAllTiers is true)
  const tiersToShow = isModal && !showAllTiers
    ? sortedTiers.filter(tier => {
        const currentIndex = sortedTiers.findIndex(t => t.tier === currentTier)
        const tierIndex = sortedTiers.findIndex(t => t.tier === tier.tier)
        return tierIndex >= currentIndex
      })
    : sortedTiers

  // Show loading state if pricing is loading
  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        <span className="ml-2 text-muted-foreground">{t('loadingPricing')}</span>
      </div>
    )
  }

  // Show error state - no fallback, require Stripe pricing
  if (error || !pricing) {
    return (
      <ErrorDisplay
        title={t('errorTitle')}
        message={t('errorDescription')}
        variant="card"
        className="my-12 text-center"
        titleClassName="justify-center"
        descriptionClassName="text-center"
      />
    )
  }

  return (
    <PlanComparisonContent 
      tiersToShow={tiersToShow}
      currentTier={currentTier}
      onUpgrade={onUpgrade}
      highlightUpgrades={highlightUpgrades}
      showPricing={showPricing}
      showCallToAction={showCallToAction}
      showUnifiedTrialButton={showUnifiedTrialButton}
      isTrialUser={isTrialUser}
      isModal={isModal}
      isLoading={isLoading}
      loadingTier={loadingTier}
      hasPaidSubscription={hasPaidSubscription}
    />
  )
}

interface PlanComparisonContentProps {
  tiersToShow: Array<{
    tier: string
    name: string
    description?: string
    monthly_price?: { price_id: string, amount: number, currency: string, interval: string }
    yearly_price?: { price_id: string, amount: number, currency: string, interval: string }
    features: Record<string, string>
  }>
  currentTier: string
  onUpgrade?: (targetTier: string, isYearly: boolean) => void
  highlightUpgrades: boolean
  showPricing: boolean
  showCallToAction: boolean
  isTrialUser: boolean
  isModal: boolean
  isLoading: boolean
  loadingTier: string | null
  showUnifiedTrialButton: boolean
  hasPaidSubscription: boolean
}

function PlanComparisonContent({
  tiersToShow,
  currentTier,
  onUpgrade,
  highlightUpgrades,
  showPricing,
  showCallToAction,
  showUnifiedTrialButton,
  isTrialUser,
  isModal,
  isLoading,
  loadingTier,
  hasPaidSubscription
}: PlanComparisonContentProps) {
  const t = useTranslations('billing')
  const locale = useLocale()

  // Separate paid tiers from self-hosted for different layout
  const paidTiers = tiersToShow.filter(tier => tier.tier !== 'selfhosted')
  const selfHostedTier = tiersToShow.find(tier => tier.tier === 'selfhosted')

  return (
    <div className="space-y-6">
{/* Removed billing toggle - always show monthly with yearly savings */}

      {/* Paid tiers grid */}
      <div className={`grid gap-4 sm:gap-6 ${paidTiers.length === 2 ? 'md:grid-cols-2' : paidTiers.length === 1 ? 'md:grid-cols-1' : 'md:grid-cols-3'} ${isModal ? 'max-w-6xl mx-auto' : 'max-w-3xl mx-auto'}`}>
      {paidTiers.map((tier) => {
        const isCurrentTier = tier.tier === currentTier
        const isUpgrade = !isCurrentTier && highlightUpgrades
        const isLoadingThisTier = isLoading && loadingTier === tier.tier
        
        // Get pricing info 
        const monthlyPrice = tier.monthly_price
        const yearlyPrice = tier.yearly_price
        
        return (
          <Card 
            key={tier.tier} 
            className={`relative ${
              isCurrentTier 
                ? "border-blue-500 bg-blue-50 shadow-md ring-2 ring-blue-200" 
                : isUpgrade 
                  ? "border-primary shadow-md" 
                  : ""
            }`}
          >
            {tier.tier === 'pro' && !isCurrentTier && (
              <div className="absolute -top-3 left-4">
                <span className="bg-primary text-primary-foreground text-xs px-2 py-1 rounded-full font-semibold">
                  {t('popular')}
                </span>
              </div>
            )}
            {isCurrentTier && !isTrialUser && (
              <div className="absolute -top-3 left-4">
                <span className="bg-blue-500 text-white text-xs px-2 py-1 rounded-full font-semibold">
                  {t('currentPlanBadge')}
                </span>
              </div>
            )}
            {isCurrentTier && isTrialUser && (
              <div className="absolute -top-3 left-4">
                <span className="bg-orange-500 text-white text-xs px-2 py-1 rounded-full font-semibold">
                  {t('trialPlanBadge')}
                </span>
              </div>
            )}
            
            <CardHeader>
              <CardTitle className="text-lg">{t(`plans.${getTierTranslationKey(tier.tier)}.name`)}</CardTitle>
              <CardDescription className="text-sm">{t(`plans.${getTierTranslationKey(tier.tier)}.description`)}</CardDescription>
              {showPricing && tier.tier === 'selfhosted' && (
                <div className="mt-3">
                  <span className="text-2xl font-bold text-green-600">{t('freeToSelfHost')}</span>
                </div>
              )}
              {showPricing && monthlyPrice && tier.tier !== 'selfhosted' && (
                <div className="mt-3">
                  <span className="text-2xl font-bold">{formatPrice(monthlyPrice.amount, monthlyPrice.currency, locale)}</span>
                  <span className="text-muted-foreground text-sm">{t('perMonth')}</span>
                  <div className="text-xs text-muted-foreground mt-0.5">
                    {t('plusTaxes')}
                  </div>
                  {yearlyPrice && (
                    <div className="text-xs text-green-600 font-medium mt-1">
                      {t('saveYearly', { percent: Math.round(((monthlyPrice.amount * 12 - yearlyPrice.amount) / (monthlyPrice.amount * 12)) * 100) })}
                    </div>
                  )}
                </div>
              )}
            </CardHeader>
            
            <CardContent>
              <ul className="space-y-2 sm:space-y-2.5">
                {allFeatures.map((feature) => {
                  const tierKey = tier.tier as 'selfhosted' | 'personal' | 'team'
                  const value = feature[tierKey]
                  const isUnique = feature.unique?.[tierKey as keyof typeof feature.unique] || false
                  const featureKey = getFeatureTranslationKey(feature.id)

                  // Skip features that are false or undefined for this tier
                  if (value === false || value === undefined) {
                    return null
                  }

                  // Get the translated feature text
                  let featureText: string
                  if (typeof value === 'string') {
                    // Features with tier-specific values (wallets, contacts, sync)
                    featureText = t(`features.${featureKey}.${tierKey}`)
                  } else {
                    // Boolean features (trial, email, sms, push, etc.)
                    featureText = t(`features.${featureKey}`)
                  }

                  return (
                    <li key={feature.id} className={`flex items-start text-sm ${isUnique && tier.tier !== 'personal' ? 'font-medium' : ''}`}>
                      <CheckCircle2 className={`h-4 w-4 mr-2 flex-shrink-0 mt-0.5 ${isUnique && tier.tier !== 'personal' ? 'text-primary' : 'text-muted-foreground'}`} />
                      <span>
                        {featureText}
                      </span>
                    </li>
                  )
                })}
              </ul>
            </CardContent>

            {onUpgrade && (!isCurrentTier || isTrialUser) && !showCallToAction && (
              <CardFooter className={isModal ? "sticky bottom-0 bg-inherit pb-4 border-t md:static md:border-t-0" : ""}>
                <Button
                  className="w-full"
                  variant={isUpgrade ? "default" : "outline"}
                  onClick={() => {
                    if (onUpgrade) {
                      onUpgrade(tier.tier, false) // Always start with monthly (upsell will be shown in Stripe)
                    }
                  }}
                  disabled={isLoadingThisTier}
                >
                  {isLoadingThisTier && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  {hasPaidSubscription ? t('changePlan') : (isTrialUser ? t('subscribeTo', { plan: t(`plans.${getTierTranslationKey(tier.tier)}.name`) }) : t('upgradeTo', { plan: t(`plans.${getTierTranslationKey(tier.tier)}.name`) }))}
                </Button>
              </CardFooter>
            )}

          </Card>
        )
      })}
      </div>

      {showUnifiedTrialButton && (
        <div className="text-center mt-8">
          <Button size="lg" asChild>
            <a href="/sign-up">
              {t('startTrial')}
            </a>
          </Button>
        </div>
      )}

      {/* Self-hosted section - shown below paid tiers on public page */}
      {selfHostedTier && showCallToAction && (
        <div className="max-w-3xl mx-auto mt-12 pt-8 border-t">
          <div className="text-center space-y-3">
            <p className="text-muted-foreground">
              {t('selfHosted.description')}
            </p>
            <div className="flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-sm">
              <a
                href="https://apps.umbrel.com/app/canary"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
              >
                <Image
                  src="/images/nodes/umbrel.svg"
                  alt="Umbrel"
                  width={16}
                  height={16}
                  className="opacity-60"
                />
                {t('selfHosted.installUmbrel')}
              </a>
              <span className="text-muted-foreground/40">•</span>
              <a
                href="https://marketplace.start9.com/canary?api=community-registry.start9.com&name=Community%20Registry"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
              >
                <Image
                  src="/images/nodes/start9.svg"
                  alt="Start9"
                  width={16}
                  height={16}
                  className="opacity-60"
                />
                {t('selfHosted.installStart9')}
              </a>
              <span className="text-muted-foreground/40">•</span>
              <a
                href="https://github.com/schjonhaug/canary"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
              >
                <Github className="h-4 w-4" />
                {t('selfHosted.viewGithub')}
              </a>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
