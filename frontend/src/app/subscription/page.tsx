"use client"

import { useEffect, useState } from "react"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Loader2, CreditCard, Users, Calendar, TrendingUp, Zap, AlertTriangle, Clock, XCircle } from "lucide-react"
import { PlansModal } from "@/components/plans-modal"
import { getTierDisplayName, getTierDescription } from "@/lib/pricing-data"
import Link from "next/link"

export default function SubscriptionPage() {
  const { user, billingStatus, isLoading, refreshBillingStatus, isSelfHostedMode } = useAuth()
  const [isPortalLoading, setIsPortalLoading] = useState(false)
  const [showUpgradeModal, setShowUpgradeModal] = useState(false)

  useEffect(() => {
    // Only refresh billing status when auth is ready and user is logged in
    if (!isLoading && user) {
      refreshBillingStatus()
    }
  }, [refreshBillingStatus, isLoading, user])

  const handleManageBilling = async () => {
    if (!billingStatus?.stripe_customer_id) return

    try {
      setIsPortalLoading(true)
      const { url } = await api.createCustomerPortalSession(window.location.origin + '/subscription')
      window.location.href = url
    } catch (error) {
      console.error('Failed to open customer portal:', error)
      alert('Failed to open billing management. Please try again.')
    } finally {
      setIsPortalLoading(false)
    }
  }


  // Hide billing page in self-hosted mode
  if (isSelfHostedMode) {
    return (
      <div className="space-y-6">
        <h2 className="text-2xl font-semibold">Subscription</h2>
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>Self-Hosted Mode</CardTitle>
            <CardDescription>
              Subscription billing is not available in self-hosted mode.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Link href="/wallets">
              <Button variant="outline" className="w-full">
                Back to Wallets
              </Button>
            </Link>
          </CardContent>
        </Card>
      </div>
    )
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
      <div className="space-y-6">
        <h2 className="text-2xl font-semibold">Subscription</h2>
        <div className="text-center py-12">
          <p className="text-muted-foreground mb-6">You need to be signed in to manage your subscription.</p>
          <Button asChild>
            <Link href="/sign-in">Sign In</Link>
          </Button>
        </div>
      </div>
    )
  }

  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
  const limits = billingStatus?.limits
  const isTrialUser = billingStatus?.subscription_status === 'trialing'
  const isCancelledUser = billingStatus?.subscription_status === 'canceled'

  // Calculate days remaining in trial
  const trialDaysRemaining = billingStatus?.trial_ends_at ?
    Math.max(0, Math.ceil((new Date(billingStatus.trial_ends_at).getTime() - new Date().getTime()) / (1000 * 60 * 60 * 24))) : 0

  // Calculate days remaining until subscription ends (for cancelled subscriptions)
  const subscriptionDaysRemaining = billingStatus?.subscription_ends_at ?
    Math.max(0, Math.ceil((new Date(billingStatus.subscription_ends_at).getTime() - new Date().getTime()) / (1000 * 60 * 60 * 24))) : 0

  // Check if limits are exceeded
  const walletCount = billingStatus?.wallet_count || 0
  const maxWallets = limits?.max_wallets || 1
  const walletsExceeded = maxWallets !== -1 && walletCount > maxWallets

  return (
    <div className="space-y-6">
      {/* Page Title */}
      <h2 className="text-2xl font-semibold">Subscription</h2>

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
            {billingStatus?.stripe_customer_id && !isTrialUser && billingStatus?.subscription_status !== 'expired' && (
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
          {walletsExceeded && (
            <div className="bg-orange-50 border border-orange-200 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <AlertTriangle className="h-5 w-5 text-orange-600" />
                <div className="font-medium text-orange-700">Subscription Limit Exceeded</div>
              </div>
              <div className="text-sm text-orange-600 mb-3">
                You have {walletCount} wallets but your {getTierDisplayName(currentTier)} plan allows only {maxWallets}.
                Excess wallets won&apos;t sync automatically and some contacts may be inactive.
              </div>
              <Button
                onClick={() => setShowUpgradeModal(true)}
                size="sm"
                className="bg-orange-600 hover:bg-orange-700 text-white"
              >
                Upgrade Plan
              </Button>
            </div>
          )}

          {/* Pending Status - Trial Not Started */}
          {billingStatus?.subscription_status === 'pending' && (
            <div className="bg-gradient-to-r from-blue-50 to-indigo-50 border border-blue-200 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <Clock className="h-5 w-5 text-blue-600" />
                <div className="font-medium text-blue-700">Trial Not Started</div>
              </div>
              <div className="text-sm text-blue-600 mb-3">
                Your 30-day Team trial will begin when you add your first wallet. No syncing is active until then.
              </div>
              <Button
                onClick={() => window.location.href = '/wallets'}
                size="sm"
                className="bg-blue-600 hover:bg-blue-700 text-white"
              >
                Add Your First Wallet
              </Button>
            </div>
          )}

          {/* Cancelled Status - Access Until Period End */}
          {billingStatus?.subscription_status === 'canceled' && subscriptionDaysRemaining > 0 && (
            <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <AlertTriangle className="h-5 w-5 text-yellow-600" />
                <div className="font-medium text-yellow-700">Subscription Cancelled</div>
              </div>
              <div className="text-sm text-yellow-600 mb-3">
                Your subscription has been cancelled. You have access until {new Date(billingStatus?.subscription_ends_at || '').toLocaleDateString()} ({subscriptionDaysRemaining} days remaining).
              </div>
              <Button
                onClick={handleManageBilling}
                disabled={isPortalLoading}
                size="sm"
                className="bg-yellow-600 hover:bg-yellow-700 text-white"
              >
                {isPortalLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Reactivate Subscription
              </Button>
            </div>
          )}

          {/* Expired Status - Subscription Ended */}
          {billingStatus?.subscription_status === 'expired' && (
            <div className="bg-red-50 border border-red-200 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <XCircle className="h-5 w-5 text-red-600" />
                <div className="font-medium text-red-700">Subscription Expired</div>
              </div>
              <div className="text-sm text-red-600 mb-3">
                Your subscription has expired. Wallet syncing has stopped completely, but your data is preserved.
              </div>
              <Button
                onClick={handleManageBilling}
                disabled={isPortalLoading}
                size="sm"
                className="bg-red-600 hover:bg-red-700 text-white"
              >
                {isPortalLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Reactivate Subscription
              </Button>
            </div>
          )}

          {/* Past Due Status - Payment Failed */}
          {billingStatus?.subscription_status === 'past_due' && (
            <div className="bg-orange-50 border border-orange-200 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <AlertTriangle className="h-5 w-5 text-orange-600" />
                <div className="font-medium text-orange-700">Payment Failed - Syncing Stopped</div>
              </div>
              <div className="text-sm text-orange-600 mb-3">
                Your last payment failed and wallet syncing has been stopped immediately. Update your payment method to resume service.
              </div>
              <Button
                onClick={handleManageBilling}
                disabled={isPortalLoading}
                size="sm"
                className="bg-orange-600 hover:bg-orange-700 text-white"
              >
                {isPortalLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Update Payment Method
              </Button>
            </div>
          )}

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
              {isCancelledUser && subscriptionDaysRemaining > 0 && (
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-yellow-600 border-yellow-600">
                    Access until: {subscriptionDaysRemaining} days left
                  </Badge>
                </div>
              )}
              {!isTrialUser && !isCancelledUser && (
                <span className="text-muted-foreground">{getTierDescription(currentTier)}</span>
              )}
            </div>
            {isTrialUser && (
              <Button onClick={() => setShowUpgradeModal(true)} className="bg-blue-600 hover:bg-blue-700">
                <Zap className="mr-2 h-4 w-4" />
                Subscribe
              </Button>
            )}
            {isCancelledUser && subscriptionDaysRemaining > 0 && (
              <Button onClick={handleManageBilling} disabled={isPortalLoading} className="bg-yellow-600 hover:bg-yellow-700">
                {isPortalLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Reactivate
              </Button>
            )}
          </div>

          {limits && (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className={`flex items-center gap-3 p-3 rounded-lg ${
                walletsExceeded ? 'bg-orange-50 border border-orange-200' : 'bg-muted/50'
              }`}>
                <div className="flex items-center gap-2">
                  <Users className={`h-5 w-5 ${walletsExceeded ? 'text-orange-600' : 'text-muted-foreground'}`} />
                  {walletsExceeded && <AlertTriangle className="h-4 w-4 text-orange-600" />}
                </div>
                <div>
                  <div className={`font-medium ${walletsExceeded ? 'text-orange-700' : ''}`}>Wallets</div>
                  <div className={`text-sm ${walletsExceeded ? 'text-orange-600 font-medium' : 'text-muted-foreground'}`}>
                    {billingStatus?.wallet_count || 0} / {limits.max_wallets === -1 ? '∞' : limits.max_wallets}
                    {walletsExceeded && <span className="ml-1 text-xs">(over limit)</span>}
                  </div>
                  {walletsExceeded && (
                    <div className="text-xs text-orange-600 mt-1">
                      Some wallets are inactive
                    </div>
                  )}
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
                    SMS, Push, Email, Transaction Analysis
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
        billingStatus={billingStatus ? {
          subscription_status: billingStatus.subscription_status,
          stripe_customer_id: billingStatus.stripe_customer_id
        } : undefined}
      />
    </div>
  )
}
