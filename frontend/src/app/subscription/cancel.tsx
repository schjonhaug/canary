"use client"

import Link from "next/link"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { XCircle, ArrowLeft, MessageCircle } from "lucide-react"
import { SUPPORT_EMAIL } from "@/lib/constants"
import { useTranslations } from "next-intl"

export default function BillingCancelPage() {
  const t = useTranslations('subscriptionCancel')

  return (
    <div className="max-w-2xl mx-auto p-6 space-y-6">
      <Card>
        <CardHeader className="text-center">
          <div className="mx-auto w-16 h-16 rounded-full bg-orange-100 flex items-center justify-center mb-4">
            <XCircle className="h-8 w-8 text-orange-600" />
          </div>
          <CardTitle className="text-2xl">{t('title')}</CardTitle>
          <CardDescription>
            {t('description')}
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-6">
          {/* What happened */}
          <div className="bg-muted/50 rounded-lg p-4 space-y-2">
            <h3 className="font-semibold">{t('whatHappenedTitle')}</h3>
            <p className="text-sm text-muted-foreground">
              {t('whatHappenedDescription')}
            </p>
          </div>

          {/* Options */}
          <div className="space-y-3">
            <h3 className="font-semibold">{t('optionsTitle')}</h3>
            <div className="grid gap-3">
              <Button asChild>
                <Link href="/subscription">
                  <ArrowLeft className="mr-2 h-4 w-4" />
                  {t('tryAgain')}
                </Link>
              </Button>

              <Button variant="outline" asChild>
                <Link href="/wallets">
                  {t('continueCurrent')}
                </Link>
              </Button>

              <Button variant="outline" asChild>
                <a href={`mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(t('mailtoSubject'))}&body=${encodeURIComponent(t('mailtoBody'))}`}>
                  <MessageCircle className="mr-2 h-4 w-4" />
                  {t('contactSupport')}
                </a>
              </Button>
            </div>
          </div>

          {/* Help section */}
          <div className="border-t pt-4 space-y-2">
            <h4 className="font-medium text-sm">{t('helpTitle')}</h4>
            <div className="text-sm text-muted-foreground space-y-1">
              <p>{'• '}{t.rich('personalDescription', {
                strong: (chunks) => <strong>{chunks}</strong>
              })}</p>
              <p>{'• '}{t.rich('teamDescription', {
                strong: (chunks) => <strong>{chunks}</strong>
              })}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Support Info */}
      <Card>
        <CardContent className="pt-6">
          <div className="text-center text-sm text-muted-foreground space-y-1">
            <p>{t('supportQuestion')}</p>
            <p>{t.rich('supportEmail', {
              email: SUPPORT_EMAIL,
              link: (chunks) => <a href={`mailto:${SUPPORT_EMAIL}`} className="text-primary hover:underline">{chunks}</a>
            })}</p>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
