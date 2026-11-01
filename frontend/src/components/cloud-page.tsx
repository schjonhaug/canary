'use client'

import { useState } from 'react'
import Image from 'next/image'
import { PrivatePromotion, PrivateLink } from '@/components/private-promotion'
import Link from 'next/link'
import { ArrowLeft, ArrowRight, Cloud, Code2, Mail, MessageSquare } from 'lucide-react'
import { useLocale, useTranslations } from 'next-intl'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PlanComparison } from '@/components/plan-comparison'
import { formatPrice, sortTiers, type PricingData, type StripePricingTier } from '@/hooks/usePricing'
import { sourceOption } from '@/lib/install-options'

const CLOUD_URL = 'https://canarybitcoin.com/cloud'
const SIGN_UP_URL = 'https://canarybitcoin.com/sign-up'

const faqKeys = ['walletData', 'visibility', 'identity', 'privateKeys', 'selfHost'] as const
const aboutKeys = ['who', 'how'] as const
const heroFeatures = [
  { key: 'email', icon: Mail },
  { key: 'sms', icon: MessageSquare },
  { key: 'noNode', icon: Cloud },
] as const
// Stripe tiers with copy in the billing namespace; the self-hosted tier is not a Stripe price.
const paidTiers = ['personal', 'team']

type Price = NonNullable<StripePricingTier['monthly_price']>

function toOffer(name: string, price: Price, unitCode: 'MON' | 'ANN') {
  const amount = (price.amount / 100).toFixed(2)
  const priceCurrency = price.currency.toUpperCase()
  return {
    '@type': 'Offer',
    name,
    price: amount,
    priceCurrency,
    availability: 'https://schema.org/InStock',
    url: SIGN_UP_URL,
    priceSpecification: {
      '@type': 'UnitPriceSpecification',
      price: amount,
      priceCurrency,
      billingDuration: 1,
      unitCode,
    },
  }
}

// Keeps a stray "</script>" in translated copy from ending the JSON-LD block early.
function serializeJsonLd(data: unknown): string {
  return JSON.stringify(data).replace(/</g, '\\u003c')
}

export default function CloudPageContent({ nonce }: { nonce?: string }) {
  const t = useTranslations('cloudPage')
  const tBilling = useTranslations('billing')
  const locale = useLocale()
  const [pricing, setPricing] = useState<PricingData | null>(null)

  const tiers = pricing ? sortTiers(pricing.tiers).filter((tier) => paidTiers.includes(tier.tier)) : []

  const planCosts = tiers.flatMap((tier) => {
    if (!tier.monthly_price || !tier.yearly_price) return []
    return [{
      tier: tier.tier,
      text: t('about.planCost', {
        plan: tBilling(`plans.${tier.tier}.name`),
        description: tBilling(`plans.${tier.tier}.description`),
        monthly: formatPrice(tier.monthly_price.amount, tier.monthly_price.currency, locale),
        yearly: formatPrice(tier.yearly_price.amount, tier.yearly_price.currency, locale),
        wallets: tBilling(`plans.${tier.tier}.features.wallets`),
        contacts: tBilling(`plans.${tier.tier}.features.contacts`),
        sync: tBilling(`plans.${tier.tier}.features.sync`),
      }),
    }]
  })

  const offers = tiers.flatMap((tier) => {
    const plan = tBilling(`plans.${tier.tier}.name`)
    return [
      ...(tier.monthly_price ? [toOffer(t('about.monthlyOffer', { plan }), tier.monthly_price, 'MON')] : []),
      ...(tier.yearly_price ? [toOffer(t('about.yearlyOffer', { plan }), tier.yearly_price, 'ANN')] : []),
    ]
  })

  const productJsonLd = {
    '@context': 'https://schema.org',
    '@type': 'Product',
    name: t('hero.title'),
    url: CLOUD_URL,
    description: t('hero.description'),
    brand: { '@type': 'Brand', name: 'Canary Wallet' },
    ...(offers.length > 0 ? { offers } : {}),
  }

  // Mirrors the visible privacy questions below, so the structured data never says more than the page.
  const faqJsonLd = {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: faqKeys.map((key) => ({
      '@type': 'Question',
      name: t(`faq.items.${key}.question`),
      acceptedAnswer: { '@type': 'Answer', text: t(`faq.items.${key}.answer`) },
    })),
  }

  return (
    <div className="min-h-screen overflow-x-hidden">
      <script nonce={nonce} type="application/ld+json" dangerouslySetInnerHTML={{ __html: serializeJsonLd(productJsonLd) }} />
      <script nonce={nonce} type="application/ld+json" dangerouslySetInnerHTML={{ __html: serializeJsonLd(faqJsonLd) }} />

      <header className="container mx-auto flex min-h-20 items-center justify-between gap-4 px-4 py-4">
        <Link href="/" className="flex items-center gap-3 transition-opacity hover:opacity-80">
          <Image src="/images/canary.svg" alt="" width={40} height={40} className="h-10 w-10" priority />
          <span className="font-bold tracking-wide">Canary Wallet</span>
        </Link>
        <nav className="flex items-center gap-2 sm:gap-4" aria-label={t('nav.label')}>
          <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="hidden items-center gap-2 text-sm text-muted-foreground hover:text-foreground sm:inline-flex"><Code2 className="h-4 w-4" />GitHub</a>
          <Button variant="ghost" size="sm" className="hidden sm:inline-flex" asChild><Link href="/demo">{t('nav.demo')}</Link></Button>
          <Button variant="outline" size="sm" asChild><Link href="/sign-in">{t('nav.signIn')}</Link></Button>
        </nav>
      </header>

      <main>
        <section className="container mx-auto px-4 pb-14 pt-14 text-center sm:pt-20">
          <div className="mx-auto max-w-3xl">
            <div className="mb-5 inline-flex items-center gap-2 rounded-full border bg-card px-3 py-1 text-sm text-muted-foreground"><Cloud className="h-4 w-4 text-primary" />{t('hero.eyebrow')}</div>
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">{t('hero.title')}</h1>
            <p className="mx-auto mt-6 max-w-2xl text-lg leading-8 text-muted-foreground">{t('hero.description')}</p>
            <ul className="mt-6 flex flex-wrap justify-center gap-2">
              {heroFeatures.map(({ key, icon: Icon }) => (
                <li key={key} className="inline-flex items-center gap-2 rounded-full border bg-card px-3 py-1 text-sm">
                  <Icon className="h-4 w-4 text-primary" />
                  {t(`hero.features.${key}`)}
                </li>
              ))}
            </ul>
            <div className="mt-8 flex flex-wrap justify-center gap-3">
              <Button size="lg" asChild><Link href="/sign-up">{t('hero.signUp')}<ArrowRight /></Link></Button>
              <Button size="lg" variant="secondary" asChild><Link href="/demo">{t('hero.demo')}</Link></Button>
            </div>
            <Button variant="link" className="mt-5" asChild><Link href="/#install"><ArrowLeft />{t('hero.selfHost')}</Link></Button>
          </div>
        </section>

        <section id="about" className="container mx-auto px-4 pb-20">
          <div className="mx-auto max-w-3xl">
            <h2 className="text-3xl font-semibold tracking-tight">{t('about.title')}</h2>
            <p className="mt-4 leading-7 text-muted-foreground">{t('about.what')}</p>
            {aboutKeys.map((key) => (
              <div key={key}>
                <h3 className="mt-10 text-xl font-semibold">{t(`about.${key}Title`)}</h3>
                <p className="mt-3 leading-7 text-muted-foreground">{t(`about.${key}`)}</p>
              </div>
            ))}
            <h3 className="mt-10 text-xl font-semibold">{t('about.costTitle')}</h3>
            <p className="mt-3 leading-7 text-muted-foreground">{t('about.cost')}</p>
            {planCosts.length > 0 && (
              <ul className="mt-3 list-disc space-y-2 pl-5 leading-7 text-muted-foreground">
                {planCosts.map(({ tier, text }) => <li key={tier}>{text}</li>)}
              </ul>
            )}
          </div>
        </section>

        <section id="pricing" className="border-y bg-muted/30">
          <div className="container mx-auto px-4 py-20">
            <div className="mb-10 text-center">
              <h2 className="text-3xl font-semibold tracking-tight">{t('pricing.title')}</h2>
              <p className="mt-3 text-muted-foreground">{t('pricing.description')}</p>
              <p className="mt-2 text-sm font-medium text-primary">{t('pricing.trial')}</p>
            </div>
            <PlanComparison
              currentTier=""
              highlightUpgrades={false}
              showPricing
              isModal={false}
              showCallToAction
              showUnifiedTrialButton
              onPricingLoaded={setPricing}
            />
          </div>
        </section>

        <PrivatePromotion />

        <section id="faq" className="container mx-auto px-4 py-20">
          <div className="mx-auto mb-10 max-w-3xl text-center">
            <h2 className="text-3xl font-semibold tracking-tight">{t('faq.title')}</h2>
            <p className="mt-3 text-muted-foreground">{t('faq.description')}</p>
          </div>
          <div className="mx-auto grid max-w-5xl gap-4 md:grid-cols-2">
            {faqKeys.map((key) => (
              <Card key={key} className="gap-3 py-5 shadow-none">
                <CardHeader className="px-5"><CardTitle className="leading-6">{t(`faq.items.${key}.question`)}</CardTitle></CardHeader>
                <CardContent className="px-5 text-sm leading-6 text-muted-foreground">{t(`faq.items.${key}.answer`)}</CardContent>
              </Card>
            ))}
          </div>
        </section>

        <section className="container mx-auto px-4 pb-20">
          <Card className="mx-auto max-w-4xl bg-primary text-primary-foreground">
            <CardContent className="py-6 text-center">
              <h2 className="text-3xl font-semibold">{t('final.title')}</h2>
              <p className="mx-auto mt-3 max-w-xl opacity-80">{t('final.description')}</p>
              <div className="mt-7 flex flex-wrap justify-center gap-3">
                <Button size="lg" variant="secondary" asChild><Link href="/sign-up">{t('final.signUp')}<ArrowRight /></Link></Button>
                <Button size="lg" variant="outline" className="border-primary-foreground bg-transparent text-primary-foreground hover:bg-primary-foreground/10 hover:text-primary-foreground" asChild><Link href="/demo">{t('final.demo')}</Link></Button>
                <Button size="lg" variant="outline" className="border-primary-foreground bg-transparent text-primary-foreground hover:bg-primary-foreground/10 hover:text-primary-foreground" asChild><Link href="/sign-in">{t('final.signIn')}</Link></Button>
              </div>
            </CardContent>
          </Card>
        </section>
      </main>

      <footer className="border-t">
        <div className="container mx-auto flex flex-wrap items-center justify-between gap-4 px-4 py-8 text-sm text-muted-foreground">
          <PrivateLink />
          <Link href="/#install" className="hover:text-foreground">{t('footer.selfHost')}</Link>
          <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-2 hover:text-foreground"><Code2 className="h-4 w-4" />GitHub</a>
        </div>
      </footer>
    </div>
  )
}
