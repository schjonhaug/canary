'use client'

import Image from 'next/image'
import Link from 'next/link'
import { ArrowLeft, ArrowRight, Cloud, Code2, Database, Eye, KeyRound, Link2 } from 'lucide-react'
import { useTranslations } from 'next-intl'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { PlanComparison } from '@/components/plan-comparison'
import { sourceOption } from '@/lib/install-options'

const disclosureItems = [
  { key: 'walletData', icon: Database },
  { key: 'visibility', icon: Eye },
  { key: 'identity', icon: Link2 },
  { key: 'privateKeys', icon: KeyRound },
] as const

export default function CloudPageContent() {
  const t = useTranslations('cloudPage')

  return (
    <div className="min-h-screen overflow-x-hidden">
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
            <Button variant="link" className="mt-5" asChild><Link href="/#install"><ArrowLeft />{t('hero.selfHost')}</Link></Button>
          </div>
        </section>

        <section className="container mx-auto px-4 pb-20">
          <Card className="mx-auto max-w-5xl border-primary/20">
            <CardHeader>
              <p className="text-sm font-medium text-primary">{t('privacy.eyebrow')}</p>
              <CardTitle className="text-2xl"><h2>{t('privacy.title')}</h2></CardTitle>
              <p className="max-w-3xl text-sm leading-6 text-muted-foreground">{t('privacy.description')}</p>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              {disclosureItems.map(({ key, icon: Icon }) => (
                <div key={key} className="flex gap-3 rounded-lg border bg-muted/30 p-4">
                  <Icon className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
                  <div><h2 className="font-semibold">{t(`privacy.items.${key}.title`)}</h2><p className="mt-1 text-sm leading-6 text-muted-foreground">{t(`privacy.items.${key}.description`)}</p></div>
                </div>
              ))}
            </CardContent>
          </Card>
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
            />
          </div>
        </section>

        <section className="container mx-auto px-4 py-20">
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
          <Link href="/#install" className="hover:text-foreground">{t('footer.selfHost')}</Link>
          <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-2 hover:text-foreground"><Code2 className="h-4 w-4" />GitHub</a>
        </div>
      </footer>
    </div>
  )
}
