"use client"

import { useEffect, useState } from "react"
import { useSearchParams } from "next/navigation"
import Link from "next/link"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { CheckCircle2, Loader2, ArrowRight } from "lucide-react"
import { getTierDisplayName } from "@/lib/pricing-data"
import { formatPrice, usePricing } from "@/hooks/usePricing"

export default function BillingSuccessPage() {
  const searchParams = useSearchParams()
  const sessionId = searchParams.get('session')
  const { refreshBillingStatus } = useAuth()
  const { pricing } = usePricing()
  const discountPercent = pricing?.yearly_discount_percent || 20

  const [sessionDetails, setSessionDetails] = useState<{
    session_id: string
    status: string
    tier?: string
    billing_period?: string
    amount_total?: number
    currency?: string
  } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const fetchSessionDetails = async () => {
      if (!sessionId) {
        setError('No session ID provided')
        setLoading(false)
        return
      }

      try {
        setLoading(true)
        const details = await api.getCheckoutSessionDetails(sessionId)
        setSessionDetails(details)

        // Refresh billing status to get updated subscription
        setTimeout(() => {
          refreshBillingStatus()
        }, 2000)
      } catch (err) {
        console.error('Failed to fetch session details:', err)
        setError('Failed to load payment details')
      } finally {
        setLoading(false)
      }
    }

    fetchSessionDetails()
  }, [sessionId, refreshBillingStatus])

  if (loading) {
    return (
      <div className="max-w-2xl mx-auto p-6">
        <Card>
          <CardContent className="flex items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
            <span className="ml-2 text-muted-foreground">Loading payment details...</span>
          </CardContent>
        </Card>
      </div>
    )
  }

  if (error || !sessionDetails) {
    return (
      <div className="max-w-2xl mx-auto p-6">
        <Card>
          <CardContent className="text-center py-12 space-y-4">
            <div className="text-red-500 text-lg">Payment Status Unknown</div>
            <p className="text-muted-foreground">
              {error || 'We could not verify your payment status. Please check your email for confirmation or contact support.'}
            </p>
            <div className="flex gap-3 justify-center">
              <Button asChild>
                <Link href="/subscription">View Subscription</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/wallets">Go to Wallets</Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  const isSuccessful = sessionDetails.status === 'complete'
  const tierName = sessionDetails.tier ? getTierDisplayName(sessionDetails.tier) : 'Unknown'
  const isYearly = sessionDetails.billing_period === 'yearly'
  const amount = sessionDetails.amount_total
  const currency = sessionDetails.currency || 'USD'

  return (
    <div className="max-w-2xl mx-auto p-6 space-y-6">
      <Card>
        <CardHeader className="text-center">
          <div className={`mx-auto w-16 h-16 rounded-full flex items-center justify-center mb-4 ${
            isSuccessful ? 'bg-green-100' : 'bg-yellow-100'
          }`}>
            <CheckCircle2 className={`h-8 w-8 ${isSuccessful ? 'text-green-600' : 'text-yellow-600'}`} />
          </div>
          <CardTitle className="text-2xl">
            {isSuccessful ? 'Payment Successful!' : 'Payment Processing'}
          </CardTitle>
          <CardDescription>
            {isSuccessful
              ? 'Your subscription has been updated successfully.'
              : 'Your payment is being processed. This may take a few minutes.'}
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-6">
          {/* Payment Summary */}
          <div className="bg-muted/50 rounded-lg p-4 space-y-3">
            <div className="flex justify-between items-center">
              <span className="font-medium">Plan</span>
              <Badge variant="secondary">
                {tierName} {isYearly ? '(Yearly)' : '(Monthly)'}
              </Badge>
            </div>

            {amount && (
              <div className="flex justify-between items-center">
                <span className="font-medium">Amount Paid</span>
                <span className="font-bold">
                  {formatPrice(amount, currency)}
                  {isYearly ? '/year' : '/month'}
                </span>
              </div>
            )}

            <div className="flex justify-between items-center">
              <span className="font-medium">Status</span>
              <Badge variant={isSuccessful ? "default" : "secondary"}>
                {isSuccessful ? 'Active' : 'Processing'}
              </Badge>
            </div>
          </div>

          {/* Next Steps */}
          <div className="space-y-3">
            <h3 className="font-semibold">What&apos;s next?</h3>
            <div className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>Your subscription has been activated</span>
              </div>
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>You now have access to {tierName} features</span>
              </div>
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>A receipt has been sent to your email</span>
              </div>
              {isYearly && (
                <div className="flex items-start gap-2">
                  <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                  <span>You&apos;re saving {Math.round(discountPercent)}% with yearly billing</span>
                </div>
              )}
            </div>
          </div>

          {/* Actions */}
          <div className="flex flex-col sm:flex-row gap-3">
            <Button asChild className="flex-1">
              <Link href="/wallets">
                Start Using Your Plan
                <ArrowRight className="ml-2 h-4 w-4" />
              </Link>
            </Button>
            <Button variant="outline" asChild>
              <Link href="/subscription">Manage Subscription</Link>
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Support Info */}
      <Card>
        <CardContent className="pt-6">
          <div className="text-center text-sm text-muted-foreground space-y-1">
            <p>Need help? Contact us at <a href="mailto:support@canarybitcoin.com" className="text-primary hover:underline">support@canarybitcoin.com</a></p>
            <p>Session ID: <code className="text-xs bg-muted px-1 py-0.5 rounded">{sessionDetails.session_id}</code></p>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
