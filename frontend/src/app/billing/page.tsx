"use client"

import { useEffect, useState } from "react"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Loader2, CreditCard, Users, Calendar, TrendingUp } from "lucide-react"
import { PlanComparison } from "@/components/plan-comparison"
import { getTierDisplayName, getTierDescription } from "@/lib/pricing-data"

export default function BillingPage() {
  const { user, billingStatus, isLoading, refreshBillingStatus } = useAuth()
  const [isPortalLoading, setIsPortalLoading] = useState(false)
  const [isUpgrading, setIsUpgrading] = useState(false)
  const [upgradingTier, setUpgradingTier] = useState<string | null>(null)

  useEffect(() => {
    // Refresh billing status when page loads
    refreshBillingStatus()
  }, [refreshBillingStatus])

  const handleManageBilling = async () => {
    if (!billingStatus?.stripe_customer_id) return

    try {
      setIsPortalLoading(true)
      const { url } = await api.createCustomerPortalSession(window.location.origin + '/billing')
      window.location.href = url
    } catch (error) {
      console.error('Failed to open customer portal:', error)
      alert('Failed to open billing management. Please try again.')
    } finally {
      setIsPortalLoading(false)
    }
  }

  const handleUpgrade = async (targetTier: string, isYearly: boolean = true) => {
    try {
      setIsUpgrading(true)
      setUpgradingTier(targetTier)

      // Create checkout session
      const { url } = await api.createCheckoutSession(targetTier, isYearly)
      
      // Redirect to Stripe checkout
      window.location.href = url

    } catch (error) {
      console.error('Failed to create checkout session:', error)
      alert('Failed to start checkout. Please try again.')
    } finally {
      setIsUpgrading(false)
      setUpgradingTier(null)
    }
  }

  const handleContactSales = () => {
    window.location.href = 'mailto:sales@canarybitcoin.com?subject=Business Plan Inquiry&body=Hi, I am interested in the Business plan for Canary. Please contact me to discuss.'
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        <span className="ml-2 text-muted-foreground">Loading billing information...</span>
      </div>
    )
  }

  if (!user) {
    return (
      <div className="max-w-4xl mx-auto p-6">
        <div className="text-center py-12">
          <h1 className="text-2xl font-bold mb-4">Sign in to view billing</h1>
          <p className="text-muted-foreground mb-6">You need to be signed in to manage your billing and subscription.</p>
          <Button asChild>
            <a href="/sign-in">Sign In</a>
          </Button>
        </div>
      </div>
    )
  }

  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
  const limits = billingStatus?.limits

  return (
    <div className="max-w-6xl mx-auto p-6 space-y-8">
      {/* Page Header */}
      <div className="text-center space-y-2">
        <h1 className="text-3xl font-bold">Billing & Subscription</h1>
        <p className="text-muted-foreground">Manage your subscription and billing settings</p>
      </div>

      {/* Current Plan Overview */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <CardTitle className="flex items-center gap-2">
                <CreditCard className="h-5 w-5" />
                Current Plan
              </CardTitle>
              <CardDescription>Your active subscription and usage</CardDescription>
            </div>
            {billingStatus?.stripe_customer_id && (
              <Button 
                variant="outline" 
                onClick={handleManageBilling}
                disabled={isPortalLoading}
              >
                {isPortalLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Manage Billing
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex items-center gap-4">
            <Badge variant="secondary" className="text-lg px-3 py-1">
              {getTierDisplayName(currentTier)}
            </Badge>
            <span className="text-muted-foreground">{getTierDescription(currentTier)}</span>
          </div>

          {limits && (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="flex items-center gap-3 p-3 rounded-lg bg-muted/50">
                <Users className="h-5 w-5 text-muted-foreground" />
                <div>
                  <div className="font-medium">Wallets</div>
                  <div className="text-sm text-muted-foreground">
                    {billingStatus?.wallet_count || 0} / {limits.max_wallets === -1 ? '∞' : limits.max_wallets}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-3 p-3 rounded-lg bg-muted/50">
                <TrendingUp className="h-5 w-5 text-muted-foreground" />
                <div>
                  <div className="font-medium">Sync Interval</div>
                  <div className="text-sm text-muted-foreground">
                    {limits.sync_interval_seconds < 60 
                      ? `${limits.sync_interval_seconds}s` 
                      : `${Math.round(limits.sync_interval_seconds / 60)}min`}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-3 p-3 rounded-lg bg-muted/50">
                <Calendar className="h-5 w-5 text-muted-foreground" />
                <div>
                  <div className="font-medium">Features</div>
                  <div className="text-sm text-muted-foreground">
                    {[
                      limits.allows_sms && 'SMS',
                      limits.allows_push && 'Push',
                      limits.allows_transaction_analysis && 'Analysis'
                    ].filter(Boolean).join(', ') || 'Email only'}
                  </div>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Upgrade Options */}
      <div className="space-y-4">
        <div className="text-center">
          <h2 className="text-2xl font-bold">Upgrade Your Plan</h2>
          <p className="text-muted-foreground">Get more wallets, faster sync, and premium features</p>
        </div>

        <PlanComparison
          currentTier={currentTier}
          onUpgrade={handleUpgrade}
          onContactSales={handleContactSales}
          highlightUpgrades={true}
          showPricing={true}
          showBillingToggle={true}
          isModal={false}
          showCallToAction={false}
          isLoading={isUpgrading}
          loadingTier={upgradingTier}
        />
      </div>
    </div>
  )
}