"use client"

import { useEffect, useState } from "react"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Loader2, CreditCard, Users, Calendar, TrendingUp, Zap } from "lucide-react"
import { PlansModal } from "@/components/plans-modal"
import { getTierDisplayName, getTierDescription } from "@/lib/pricing-data"

export default function BillingPage() {
  const { user, billingStatus, isLoading, refreshBillingStatus } = useAuth()
  const [isPortalLoading, setIsPortalLoading] = useState(false)
  const [showUpgradeModal, setShowUpgradeModal] = useState(false)

  useEffect(() => {
    // Refresh billing status when page loads
    refreshBillingStatus()
  }, [refreshBillingStatus])

  const handleManageBilling = async () => {
    if (!billingStatus?.stripe_customer_id) return

    try {
      setIsPortalLoading(true)
      const { url } = await api.createCustomerPortalSession(window.location.origin + '/settings/subscription')
      window.location.href = url
    } catch (error) {
      console.error('Failed to open customer portal:', error)
      alert('Failed to open billing management. Please try again.')
    } finally {
      setIsPortalLoading(false)
    }
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
  const isTrialUser = billingStatus?.subscription_status === 'trialing'
  
  // Calculate days remaining in trial
  const trialDaysRemaining = billingStatus?.trial_ends_at ? 
    Math.max(0, Math.ceil((new Date(billingStatus.trial_ends_at).getTime() - new Date().getTime()) / (1000 * 60 * 60 * 24))) : 0

  return (
    <div className="max-w-6xl mx-auto p-6 space-y-8">
      {/* Page Header */}
      <div className="text-center space-y-2">
        <h1 className="text-3xl font-bold">Subscription</h1>
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
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <Badge variant="secondary" className="text-lg px-3 py-1">
                {getTierDisplayName(currentTier)}
              </Badge>
              {isTrialUser && (
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-orange-600 border-orange-600">
                    Trial: {trialDaysRemaining} days left
                  </Badge>
                </div>
              )}
              {!isTrialUser && (
                <span className="text-muted-foreground">{getTierDescription(currentTier)}</span>
              )}
            </div>
            {isTrialUser && (
              <Button onClick={() => setShowUpgradeModal(true)} className="bg-blue-600 hover:bg-blue-700">
                <Zap className="mr-2 h-4 w-4" />
                Subscribe
              </Button>
            )}
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


      {/* Plans Modal */}
      <PlansModal
        isOpen={showUpgradeModal}
        onClose={() => setShowUpgradeModal(false)}
        currentTier={currentTier}
        currentWalletCount={billingStatus?.wallet_count || 0}
        currentContactCount={billingStatus?.contact_count || 0}
        limitType="wallets"
        isTrialUser={isTrialUser}
      />
    </div>
  )
}