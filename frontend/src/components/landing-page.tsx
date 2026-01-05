'use client'

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Zap, Shield, Bell, ArrowRight, TrendingUp, TrendingDown, Target, CheckCircle } from "lucide-react"
import Link from "next/link"
import Image from "next/image"
import { PlanComparison } from "./plan-comparison"
import { useTranslations } from "next-intl"


const faqs = [
  {
    question: "What is Canary Bitcoin wallet monitoring?",
    answer: "Canary is a professional Bitcoin wallet monitoring and notification service that watches your Bitcoin wallets using XPUBs or descriptors and sends instant alerts for all transactions through email, SMS and push notifications. Perfect for monitoring cold storage, hardware wallets, and watch-only Bitcoin addresses."
  },
  {
    question: "How does Bitcoin wallet monitoring work without private keys?",
    answer: "Canary uses your wallet's XPUB (extended public key) or descriptor for watch-only Bitcoin monitoring. This gives us read-only access to track your Bitcoin addresses and detect transactions. We never have access to your private keys and cannot move your Bitcoin - ensuring complete security for your cold storage monitoring."
  },
  {
    question: "What is XPUB wallet monitoring?",
    answer: "XPUB monitoring allows you to track all addresses in your Bitcoin wallet without exposing private keys. By providing your wallet's XPUB or descriptor, Canary can monitor unlimited addresses, detect transactions at any depth (even 200+ addresses deep), and alert you instantly when Bitcoin moves."
  },
  {
    question: "Can I monitor Bitcoin cold storage wallets?",
    answer: "Yes! Canary is perfect for Bitcoin cold storage monitoring. Whether you're using hardware wallets like Ledger or Trezor, paper wallets, or any other cold storage solution, simply provide your XPUB or descriptor for secure watch-only monitoring with instant notifications."
  },
  {
    question: "How fast are Bitcoin transaction notifications?",
    answer: "With our Team plan, you get near real-time Bitcoin transaction notifications with sync intervals as fast as 2 minutes on mainnet. Most notifications arrive within seconds of transaction detection. Personal plans sync every 10 minutes, still ensuring timely alerts for your Bitcoin wallets."
  },
  {
    question: "What Bitcoin wallet types are supported?",
    answer: "Canary supports all major Bitcoin wallet types including Native SegWit (P2WPKH), Legacy (P2PKH), Nested SegWit (P2SH), and Taproot (P2TR) addresses. We automatically detect your wallet type from the XPUB or you can specify it manually. Multi-signature and hardware wallet monitoring are fully supported."
  },
  {
    question: "Why is it called Canary?",
    answer: "Like a canary in a coal mine providing early warning, Canary acts as your Bitcoin wallet's early warning system. When your Bitcoin is in cold storage, you rarely check on it. Canary monitors 24/7 and alerts you instantly when your Bitcoin moves, providing peace of mind for long-term holders."
  },
  {
    question: "Can I track Bitcoin without downloading the blockchain?",
    answer: "Yes! Canary connects to professional Electrum servers to monitor the Bitcoin blockchain for you. No need to run a full node or download the blockchain - just provide your XPUB and start receiving instant notifications for all Bitcoin wallet activity."
  },
  {
    question: "How do Bitcoin balance alerts work?",
    answer: "Canary's balance alert system lets you set custom thresholds to monitor your total wallet balance. Create alerts for when your balance goes above a certain amount, below a threshold (low balance warning), or equals zero (wallet drain detection). Perfect for tracking cold storage without manual checking."
  },
  {
    question: "What balance alert types are available?",
    answer: "You can create three types of balance alerts: 'Above' alerts trigger when your total balance exceeds a threshold (monitor when holdings reach milestones), 'Below' alerts warn when total balance drops under an amount (low balance warnings), and 'Equals' alerts detect exact balance amounts like wallet drains (balance = 0) or specific targets."
  },
  {
    question: "Can I monitor multiple Bitcoin wallets with different balance thresholds?",
    answer: "Yes! Each Bitcoin wallet can have its own set of custom balance alerts. With our Team plan, you can monitor up to 5 wallets, each with unique balance thresholds. Perfect for Uncle Jims monitoring family Bitcoin holdings or businesses tracking multiple treasury wallets with different alert requirements."
  },
  {
    question: "How can I contact you?",
    answer: "We'd love to hear from you! You can reach us through our contact form for any questions, feedback, or support requests. We typically respond within 24 hours."
  }
]

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
                  <div className="text-2xl font-bold font-mono text-blue-600">0.05 BTC</div>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <div className="flex items-center justify-between p-3 bg-green-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <CheckCircle className="h-4 w-4 text-green-600" />
                        <span className="text-sm">{t('balanceAlerts.example.aboveThreshold')}</span>
                      </div>
                      <span className="text-xs text-green-600 font-medium">ACTIVE</span>
                    </div>

                    <div className="flex items-center justify-between p-3 bg-orange-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <CheckCircle className="h-4 w-4 text-orange-600" />
                        <span className="text-sm">{t('balanceAlerts.example.belowThreshold')}</span>
                      </div>
                      <span className="text-xs text-orange-600 font-medium">ACTIVE</span>
                    </div>

                    <div className="flex items-center justify-between p-3 bg-red-50 rounded-lg">
                      <div className="flex items-center gap-2">
                        <Target className="h-4 w-4 text-red-600" />
                        <span className="text-sm">{t('balanceAlerts.example.equalsZero')}</span>
                      </div>
                      <span className="text-xs text-red-600 font-medium">ACTIVE</span>
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
            {faqs.map((faq, index) => (
              <Card key={index}>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">
                    {faq.question === "How can I contact you?" ? (
                      <Link href="/contact" className="hover:text-primary transition-colors">
                        {faq.question}
                      </Link>
                    ) : (
                      faq.question
                    )}
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-sm text-muted-foreground">
                    {faq.answer}
                    {faq.question === "How can I contact you?" && (
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