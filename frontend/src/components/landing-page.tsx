'use client'

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Zap, Shield, Bell, ArrowRight, TrendingUp, TrendingDown, Target, CheckCircle } from "lucide-react"
import Link from "next/link"
import Image from "next/image"
import { PlanComparison } from "./plan-comparison"
import { useTranslations } from "next-intl"


// FAQ keys matching the translation file structure
const faqKeys = [
  'whatIsCanary',
  'howDoesItWork',
  'whatIsXpub',
  'coldStorage',
  'notificationSpeed',
  'walletTypes',
  'whyCanary',
  'noBlockchain',
  'balanceAlerts',
  'alertTypes',
  'multipleWallets',
  'contact'
] as const

export default function LandingPage() {
  const t = useTranslations('landing')
  const tNav = useTranslations('nav')

  const features = [
    {
      icon: <Bell className="h-5 w-5" />,
      titleKey: 'features.notifications.title',
      descriptionKey: 'features.notifications.description'
    },
    {
      icon: <TrendingUp className="h-5 w-5" />,
      titleKey: 'features.balanceAlerts.title',
      descriptionKey: 'features.balanceAlerts.description'
    },
    {
      icon: <Shield className="h-5 w-5" />,
      titleKey: 'features.security.title',
      descriptionKey: 'features.security.description'
    },
    {
      icon: <Zap className="h-5 w-5" />,
      titleKey: 'features.sync.title',
      descriptionKey: 'features.sync.description'
    }
  ]

  return (
    <div className="min-h-screen">
      {/* Header */}
      <header className="container mx-auto px-4 py-6">
        <div className="flex justify-end">
          <Link href="/sign-in" className="text-sm text-muted-foreground hover:text-foreground">
            {tNav('alreadyUser')}
          </Link>
        </div>
      </header>

      {/* Hero Section */}
      <section className="container mx-auto px-4 py-16">
        <div className="text-center max-w-3xl mx-auto">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={200}
            height={200}
            className="mx-auto mb-6"
          />
          <h1 className="text-4xl font-bold tracking-tight mb-4">
            {t('hero.title')}
          </h1>
          <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
            {t('hero.description')}
          </p>
          <div className="flex gap-3 justify-center flex-wrap">
            <Button size="lg" asChild>
              <Link href="/sign-up">
                {t('hero.startTrial')} <ArrowRight className="ml-2 h-4 w-4" />
              </Link>
            </Button>
            <Button size="lg" variant="secondary" asChild>
              <Link href="/demo">
                {t('hero.tryDemo')}
              </Link>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <Link href="#pricing">
                {t('hero.viewPricing')}
              </Link>
            </Button>
          </div>
        </div>
      </section>


      {/* Features Grid */}
      <section className="container mx-auto px-4 py-12">
        <div className="text-center mb-10">
          <h2 className="text-2xl font-semibold mb-3">{t('features.title')}</h2>
          <p className="text-muted-foreground">
            {t('features.subtitle')}
          </p>
        </div>
        <div className="grid md:grid-cols-2 gap-4 max-w-4xl mx-auto">
          {features.map((feature, index) => (
            <Card key={index} className="border-muted">
              <CardHeader className="pb-3">
                <div className="flex items-center gap-3">
                  <div className="text-primary">{feature.icon}</div>
                  <CardTitle className="text-base">{t(feature.titleKey)}</CardTitle>
                </div>
              </CardHeader>
              <CardContent>
                <p className="text-sm text-muted-foreground">{t(feature.descriptionKey)}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </section>

      {/* Balance Alerts Feature Highlight */}
      <section className="container mx-auto px-4 py-16 bg-muted/30">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-12">
            <h2 className="text-3xl font-bold mb-4">{t('balanceAlerts.title')}</h2>
            <p className="text-lg text-muted-foreground max-w-3xl mx-auto">
              {t('balanceAlerts.description')}
            </p>
          </div>

          <div className="grid lg:grid-cols-2 gap-12 items-center">
            {/* Alert Types */}
            <div className="space-y-6">
              <Card className="border-green-200 bg-green-50">
                <CardContent className="pt-6">
                  <div className="flex items-center gap-3 mb-3">
                    <TrendingUp className="h-5 w-5 text-green-600" />
                    <h3 className="font-semibold text-green-800">{t('balanceAlerts.above.title')}</h3>
                  </div>
                  <p className="text-sm text-green-700">
                    {t('balanceAlerts.above.description')}
                  </p>
                </CardContent>
              </Card>

              <Card className="border-orange-200 bg-orange-50">
                <CardContent className="pt-6">
                  <div className="flex items-center gap-3 mb-3">
                    <TrendingDown className="h-5 w-5 text-orange-600" />
                    <h3 className="font-semibold text-orange-800">{t('balanceAlerts.below.title')}</h3>
                  </div>
                  <p className="text-sm text-orange-700">
                    {t('balanceAlerts.below.description')}
                  </p>
                </CardContent>
              </Card>

              <Card className="border-red-200 bg-red-50">
                <CardContent className="pt-6">
                  <div className="flex items-center gap-3 mb-3">
                    <Target className="h-5 w-5 text-red-600" />
                    <h3 className="font-semibold text-red-800">{t('balanceAlerts.drain.title')}</h3>
                  </div>
                  <p className="text-sm text-red-700">
                    {t('balanceAlerts.drain.description')}
                  </p>
                </CardContent>
              </Card>
            </div>

            {/* Visual Example */}
            <div className="space-y-4">
              <Card className="border-2 border-blue-200">
                <CardHeader>
                  <CardTitle className="text-lg">{t('balanceAlerts.example.title')}</CardTitle>
                  <div className="text-2xl font-bold font-mono text-blue-600">{t('balanceAlerts.example.currentBalance')}</div>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <div className="flex items-center justify-between p-3 bg-green-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <CheckCircle className="h-4 w-4 text-green-600" />
                        <span className="text-sm">{t('balanceAlerts.example.aboveThreshold')}</span>
                      </div>
                      <span className="text-xs text-green-600 font-medium">{t('balanceAlerts.example.active')}</span>
                    </div>

                    <div className="flex items-center justify-between p-3 bg-orange-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <CheckCircle className="h-4 w-4 text-orange-600" />
                        <span className="text-sm">{t('balanceAlerts.example.belowThreshold')}</span>
                      </div>
                      <span className="text-xs text-orange-600 font-medium">{t('balanceAlerts.example.active')}</span>
                    </div>

                    <div className="flex items-center justify-between p-3 bg-red-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <Target className="h-4 w-4 text-red-600" />
                        <span className="text-sm">{t('balanceAlerts.example.equalsZero')}</span>
                      </div>
                      <span className="text-xs text-red-600 font-medium">{t('balanceAlerts.example.active')}</span>
                    </div>
                  </div>

                  <div className="mt-4 pt-4 border-t">
                    <p className="text-xs text-muted-foreground text-center">
                      {t('balanceAlerts.example.deliveryNote')}
                    </p>
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </div>
      </section>

      {/* Pricing Section */}
      <section className="container mx-auto px-4 py-16" id="pricing">
        <div className="text-center mb-10">
          <h2 className="text-2xl font-semibold mb-3">{t('pricing.title')}</h2>
          <p className="text-muted-foreground mb-2">
            {t('pricing.subtitle')}
          </p>
          <p className="text-sm text-green-600 font-medium">
            ✓ {t('pricing.trialNote')}
          </p>
        </div>

        <PlanComparison
          currentTier=""
          highlightUpgrades={false}
          showPricing={true}
          isModal={false}
          showCallToAction={true}
          showUnifiedTrialButton={true}
        />
      </section>


      {/* FAQ Section */}
      <section className="container mx-auto px-4 py-12">
        <div className="max-w-3xl mx-auto">
          <div className="text-center mb-10">
            <h2 className="text-2xl font-semibold mb-3">{t('faq.title')}</h2>
          </div>
          <div className="space-y-4">
            {faqKeys.map((key) => (
              <Card key={key}>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">
                    {key === 'contact' ? (
                      <Link href="/contact" className="hover:text-primary transition-colors">
                        {t(`faq.items.${key}.question`)}
                      </Link>
                    ) : (
                      t(`faq.items.${key}.question`)
                    )}
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-sm text-muted-foreground">
                    {t(`faq.items.${key}.answer`)}
                    {key === 'contact' && (
                      <>
                        {" "}
                        <Link href="/contact" className="text-primary hover:underline">
                          {t('faq.contactUs')}
                        </Link>
                        .
                      </>
                    )}
                  </p>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      </section>

      {/* CTA Section */}
      <section className="container mx-auto px-4 py-16">
        <Card className="bg-primary text-primary-foreground max-w-3xl mx-auto">
          <CardContent className="text-center py-10">
            <h2 className="text-2xl font-semibold mb-3">
              {t('cta.title')}
            </h2>
            <p className="mb-6 opacity-90">
              {t('cta.description')}
            </p>
            <div className="flex gap-3 justify-center flex-wrap">
              <Button size="lg" variant="secondary" asChild>
                <Link href="/sign-up">
                  {t('hero.startTrial')} <ArrowRight className="ml-2 h-4 w-4" />
                </Link>
              </Button>
              <Button size="lg" variant="outline" className="bg-transparent border-primary-foreground text-primary-foreground hover:bg-primary-foreground/10" asChild>
                <Link href="/demo">
                  {t('hero.tryDemo')}
                </Link>
              </Button>
              <Button size="lg" variant="outline" className="bg-transparent border-primary-foreground text-primary-foreground hover:bg-primary-foreground/10" asChild>
                <Link href="/sign-in">
                  {tNav('signIn')}
                </Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </section>
    </div>
  )
}