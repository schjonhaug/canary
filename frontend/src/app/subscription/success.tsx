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
import { SUPPORT_EMAIL } from "@/lib/constants"
import { useTranslations, useLocale } from "next-intl"

export default function BillingSuccessPage() {
  const searchParams = useSearchParams()
  const sessionId = searchParams.get('session')
  const provider = searchParams.get('provider')
  const tierFromParams = searchParams.get('tier')
  const billingPeriodFromParams = searchParams.get('billing_period')
  const { refreshBillingStatus } = useAuth()
  const { pricing } = usePricing()
  const discountPercent = pricing?.yearly_discount_percent || 20
  const locale = useLocale()
  const t = useTranslations('subscriptionSuccess')

  const [sessionDetails, setSessionDetails] = useState<{
    status: string
    tier?: string
    billing_period?: string
    amount_total?: number
    currency?: string
  } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let isMounted = true
    let timeoutId: ReturnType<typeof setTimeout>

    const fetchSessionDetails = async () => {
      if (!sessionId) {
        if (provider === 'btcpay' && tierFromParams) {
          if (!isMounted) return
          setSessionDetails({
            status: 'complete',
            tier: tierFromParams,
            billing_period: billingPeriodFromParams || 'monthly',
          })
          timeoutId = setTimeout(() => {
            if (isMounted) {
              refreshBillingStatus()
            }
          }, 2000)
          setLoading(false)
          return
        }

        if (isMounted) {
          setError('no_session')
          setLoading(false)
        }
        return
      }

      try {
        setLoading(true)
        const details = await api.getCheckoutSessionDetails(sessionId)
        if (!isMounted) return

        setSessionDetails(details)

        // Refresh billing status to get updated subscription
        timeoutId = setTimeout(() => {
          if (isMounted) {
            refreshBillingStatus()
          }
        }, 2000)
      } catch (err) {
        console.error('Failed to fetch session details:', err)
        if (isMounted) {
          setError('load_failed')
        }
      } finally {
        if (isMounted) {
          setLoading(false)
        }
      }
    }

    fetchSessionDetails()
    return () => {
      isMounted = false
      clearTimeout(timeoutId)
    }
  }, [billingPeriodFromParams, provider, refreshBillingStatus, sessionId, tierFromParams])

  if (loading) {
    return (
      <div className="max-w-2xl mx-auto p-6">
        <Card>
          <CardContent className="flex items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
            <span className="ml-2 text-muted-foreground">{t('loading')}</span>
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
            <div className="text-red-500 text-lg">{t('errorTitle')}</div>
            <p className="text-muted-foreground">
              {t('errorDescription')}
            </p>
            <div className="flex gap-3 justify-center">
              <Button asChild>
                <Link href="/subscription">{t('viewSubscription')}</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/wallets">{t('goToWallets')}</Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  const isSuccessful = sessionDetails.status === 'complete'
  const tierName = sessionDetails.tier ? getTierDisplayName(sessionDetails.tier) : t('unknownPlan')
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
            {isSuccessful ? t('titleSuccess') : t('titleProcessing')}
          </CardTitle>
          <CardDescription>
            {isSuccessful
              ? t('descriptionSuccess')
              : t('descriptionProcessing')}
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-6">
          {/* Payment Summary */}
          <div className="bg-muted/50 rounded-lg p-4 space-y-3">
            <div className="flex justify-between items-center">
              <span className="font-medium">{t('plan')}</span>
              <Badge variant="secondary">
                {tierName} {isYearly ? t('yearly') : t('monthly')}
              </Badge>
            </div>

            {amount && (
              <div className="flex justify-between items-center">
                <span className="font-medium">{t('amountPaid')}</span>
                <span className="font-bold">
                  {formatPrice(amount, currency, locale)}
                  {isYearly ? t('perYear') : t('perMonth')}
                </span>
              </div>
            )}

            <div className="flex justify-between items-center">
              <span className="font-medium">{t('status')}</span>
              <Badge variant={isSuccessful ? "default" : "secondary"}>
                {isSuccessful ? t('statusActive') : t('statusProcessing')}
              </Badge>
            </div>
          </div>

          {/* Next Steps */}
          <div className="space-y-3">
            <h3 className="font-semibold">{t('whatsNext')}</h3>
            <div className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>{t('activated')}</span>
              </div>
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>{t('accessFeatures', { tierName })}</span>
              </div>
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                <span>{t('receiptSent')}</span>
              </div>
              {isYearly && (
                <div className="flex items-start gap-2">
                  <CheckCircle2 className="h-4 w-4 mt-0.5 text-green-500" />
                  <span>{t('savingYearly', { percent: Math.round(discountPercent) })}</span>
                </div>
              )}
            </div>
          </div>

          {/* Actions */}
          <div className="flex flex-col sm:flex-row gap-3">
            <Button asChild className="flex-1">
              <Link href="/wallets">
                {t('startUsing')}
                <ArrowRight className="ml-2 h-4 w-4" />
              </Link>
            </Button>
            <Button variant="outline" asChild>
              <Link href="/subscription">{t('manageSubscription')}</Link>
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Support Info */}
      <Card>
        <CardContent className="pt-6">
          <div className="text-center text-sm text-muted-foreground">
            <p>{t.rich('supportMessage', {
              email: SUPPORT_EMAIL,
              link: (chunks) => <a href={`mailto:${SUPPORT_EMAIL}`} className="text-primary hover:underline">{chunks}</a>
            })}</p>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
