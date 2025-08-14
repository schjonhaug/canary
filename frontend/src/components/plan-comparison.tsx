"use client"

import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { CheckCircle2, Loader2 } from "lucide-react"
import { allFeatures, getTierDisplayName, getTierDescription } from "@/lib/pricing-data"
import { usePricing, formatPrice, sortTiers } from "@/hooks/usePricing"

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
  
  // Only use Stripe pricing
  const sortedTiers = pricing ? sortTiers(pricing.tiers) : []
  
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
        <span className="ml-2 text-muted-foreground">Loading pricing...</span>
      </div>
    )
  }

  // Show error state - no fallback, require Stripe pricing
  if (error || !pricing) {
    return (
      <div className="text-center py-12 bg-red-50 border border-red-200 rounded-lg">
        <p className="text-red-700 font-semibold mb-2">Unable to load pricing</p>
        <p className="text-red-600 text-sm">Please refresh the page or try again later.</p>
      </div>
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
  return (
    <div className="space-y-6">
{/* Removed billing toggle - always show monthly with yearly savings */}
      
      <div className={`grid gap-6 ${tiersToShow.length === 3 ? 'md:grid-cols-3' : tiersToShow.length === 2 ? 'md:grid-cols-2' : 'md:grid-cols-1'} ${isModal ? 'max-w-6xl mx-auto' : 'max-w-5xl mx-auto'}`}>
      {tiersToShow.map((tier) => {
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
                  POPULAR
                </span>
              </div>
            )}
            {isCurrentTier && !isTrialUser && (
              <div className="absolute -top-3 left-4">
                <span className="bg-blue-500 text-white text-xs px-2 py-1 rounded-full font-semibold">
                  CURRENT PLAN
                </span>
              </div>
            )}
            {isCurrentTier && isTrialUser && (
              <div className="absolute -top-3 left-4">
                <span className="bg-orange-500 text-white text-xs px-2 py-1 rounded-full font-semibold">
                  TRIAL PLAN
                </span>
              </div>
            )}
            
            <CardHeader>
              <CardTitle className="text-lg">{getTierDisplayName(tier.tier)}</CardTitle>
              <CardDescription className="text-sm">{tier.description || getTierDescription(tier.tier)}</CardDescription>
              {showPricing && monthlyPrice && (
                <div className="mt-3">
                  <span className="text-2xl font-bold">{formatPrice(monthlyPrice.amount, monthlyPrice.currency)}</span>
                  <span className="text-muted-foreground text-sm">/month</span>
                  {yearlyPrice && (
                    <div className="text-xs text-green-600 font-medium mt-1">
                      Save {Math.round(((monthlyPrice.amount * 12 - yearlyPrice.amount) / (monthlyPrice.amount * 12)) * 100)}% with yearly billing
                    </div>
                  )}
                </div>
              )}
            </CardHeader>
            
            <CardContent>
              <ul className="space-y-2.5">
                {allFeatures.map((feature) => {
                  const tierKey = tier.tier as 'personal' | 'team'
                  const value = feature[tierKey]
                  const isUnique = feature.unique?.[tierKey as keyof typeof feature.unique] || false
                  
                  if (value === false) {
                    return (
                      <li key={feature.id} className="flex items-start text-sm text-muted-foreground/50">
                        <span className="w-4 h-4 mr-2 flex-shrink-0 mt-0.5">–</span>
                        <span className="line-through">{feature.label}</span>
                      </li>
                    )
                  }
                  
                  return (
                    <li key={feature.id} className={`flex items-start text-sm ${isUnique && tier.tier !== 'personal' ? 'font-medium' : ''}`}>
                      <CheckCircle2 className={`h-4 w-4 mr-2 flex-shrink-0 mt-0.5 ${isUnique && tier.tier !== 'personal' ? 'text-primary' : 'text-muted-foreground'}`} />
                      <span>
                        {typeof value === 'string' ? value : feature.label}
                      </span>
                    </li>
                  )
                })}
              </ul>
            </CardContent>
            
            
            {onUpgrade && (!isCurrentTier || isTrialUser) && !showCallToAction && (
              <CardFooter>
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
                  {hasPaidSubscription ? 'Change Plan' : (isTrialUser ? `Subscribe to ${getTierDisplayName(tier.tier)}` : `Upgrade to ${getTierDisplayName(tier.tier)}`)}
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
              Start 30-Day Free Trial
            </a>
          </Button>
        </div>
      )}
    </div>
  )
}