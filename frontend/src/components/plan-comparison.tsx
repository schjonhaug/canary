"use client"

import { useState } from "react"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { CheckCircle2 } from "lucide-react"
import { allFeatures, pricingTiers, type TierSlug } from "@/lib/pricing-data"
import { BillingToggle } from "./billing-toggle"

interface PlanComparisonProps {
  currentTier: string
  onUpgrade?: (targetTier: string) => void
  onContactSales?: () => void
  highlightUpgrades?: boolean
  showPricing?: boolean
  showBillingToggle?: boolean
  isModal?: boolean
  showCallToAction?: boolean
}

export function PlanComparison({ 
  currentTier, 
  onUpgrade, 
  onContactSales,
  highlightUpgrades = true,
  showPricing = true,
  showBillingToggle = true,
  isModal = false,
  showCallToAction = false
}: PlanComparisonProps) {
  const [isYearly, setIsYearly] = useState(true)
  // Filter tiers to show only current tier and higher tiers for modal
  const tiersToShow = isModal 
    ? pricingTiers.filter(tier => {
        const currentIndex = pricingTiers.findIndex(t => t.slug === currentTier)
        const tierIndex = pricingTiers.findIndex(t => t.slug === tier.slug)
        return tierIndex >= currentIndex
      })
    : pricingTiers

  return (
    <div className="space-y-6">
      {showBillingToggle && showPricing && (
        <BillingToggle 
          isYearly={isYearly} 
          onToggle={setIsYearly} 
          className="mt-6"
        />
      )}
      
      <div className={`grid gap-6 ${tiersToShow.length === 3 ? 'md:grid-cols-3' : tiersToShow.length === 2 ? 'md:grid-cols-2' : 'md:grid-cols-1'} ${isModal ? 'max-w-6xl mx-auto' : 'max-w-5xl mx-auto'}`}>
      {tiersToShow.map((tier) => {
        const isCurrentTier = tier.slug === currentTier
        const isUpgrade = !isCurrentTier && highlightUpgrades
        
        return (
          <Card 
            key={tier.slug} 
            className={`relative ${
              isCurrentTier 
                ? "border-blue-500 bg-blue-50 shadow-md ring-2 ring-blue-200" 
                : isUpgrade 
                  ? "border-primary shadow-md" 
                  : ""
            }`}
          >
            {tier.badge && !isCurrentTier && (
              <div className="absolute -top-3 left-4">
                <span className="bg-primary text-primary-foreground text-xs px-2 py-1 rounded-full font-semibold">
                  {tier.badge}
                </span>
              </div>
            )}
            {isCurrentTier && (
              <div className="absolute -top-3 left-4">
                <span className="bg-blue-500 text-white text-xs px-2 py-1 rounded-full font-semibold">
                  CURRENT PLAN
                </span>
              </div>
            )}
            
            <CardHeader>
              <CardTitle className="text-lg">{tier.name}</CardTitle>
              <CardDescription className="text-sm">{tier.description}</CardDescription>
              {showPricing && (
                <div className="mt-3">
                  {isYearly ? (
                    <>
                      <span className="text-2xl font-bold">${tier.yearlyPrice}</span>
                      <span className="text-muted-foreground text-sm">/year</span>
                      <div className="text-xs text-muted-foreground mt-1 line-through">
                        ${tier.monthlyPrice * 12}/year
                      </div>
                    </>
                  ) : (
                    <>
                      <span className="text-2xl font-bold">${tier.monthlyPrice}</span>
                      <span className="text-muted-foreground text-sm">/month</span>
                      <div className="text-xs text-muted-foreground mt-1">
                        ${tier.yearlyPrice}/year (save 20%)
                      </div>
                    </>
                  )}
                </div>
              )}
            </CardHeader>
            
            <CardContent>
              <ul className="space-y-2.5">
                {allFeatures.map((feature) => {
                  const tierKey = tier.slug as 'personal' | 'pro' | 'business'
                  const value = feature[tierKey]
                  const isUnique = feature.unique?.[tierKey] || false
                  
                  if (value === false) {
                    return (
                      <li key={feature.id} className="flex items-start text-sm text-muted-foreground/50">
                        <span className="w-4 h-4 mr-2 flex-shrink-0 mt-0.5">–</span>
                        <span className="line-through">{feature.label}</span>
                      </li>
                    )
                  }
                  
                  return (
                    <li key={feature.id} className={`flex items-start text-sm ${isUnique && tier.slug !== 'personal' ? 'font-medium' : ''}`}>
                      <CheckCircle2 className={`h-4 w-4 mr-2 flex-shrink-0 mt-0.5 ${isUnique && tier.slug !== 'personal' ? 'text-primary' : 'text-muted-foreground'}`} />
                      <span>
                        {typeof value === 'string' ? value : feature.label}
                      </span>
                    </li>
                  )
                })}
              </ul>
            </CardContent>
            
            {showCallToAction && (
              <CardFooter>
                <Button 
                  className="w-full" 
                  variant={tier.highlighted ? "default" : "outline"}
                  asChild={tier.cta !== "Contact Sales"}
                  onClick={tier.cta === "Contact Sales" ? onContactSales : undefined}
                >
                  {tier.cta === "Contact Sales" ? (
                    tier.cta
                  ) : (
                    <a href={tier.ctaLink}>
                      {tier.cta}
                    </a>
                  )}
                </Button>
              </CardFooter>
            )}
            
            {onUpgrade && !isCurrentTier && !showCallToAction && (
              <CardFooter>
                <Button 
                  className="w-full" 
                  variant={isUpgrade ? "default" : "outline"}
                  onClick={() => {
                    if (tier.slug === "business" && onContactSales) {
                      onContactSales()
                    } else if (onUpgrade) {
                      onUpgrade(tier.slug)
                    }
                  }}
                >
                  {tier.slug === "business" ? "Contact Sales" : `Upgrade to ${tier.name}`}
                </Button>
              </CardFooter>
            )}
            
            {isCurrentTier && !showCallToAction && (
              <CardFooter>
                <Button 
                  className="w-full" 
                  variant="outline"
                  disabled
                >
                  Current Plan
                </Button>
              </CardFooter>
            )}
          </Card>
        )
      })}
      </div>
    </div>
  )
}