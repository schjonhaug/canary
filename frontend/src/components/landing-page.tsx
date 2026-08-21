'use client'

import Image from 'next/image'
import Link from 'next/link'
import {
  ArrowRight,
  Bell,
  Bitcoin,
  CircleDollarSign,
  Cloud,
  Code2,
  ExternalLink,
  Globe2,
  Heart,
  KeyRound,
  Menu,
  Radio,
  Server,
  Shield,
  WalletCards,
  Webhook,
} from 'lucide-react'
import { useTranslations } from 'next-intl'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { installOptions, sourceOption, type InstallOptionId } from '@/lib/install-options'

const trustItems = [
  { key: 'local', icon: Server },
  { key: 'keys', icon: KeyRound },
  { key: 'choice', icon: Bell },
  { key: 'cloud', icon: Cloud },
] as const

const steps = [
  { key: 'install', number: '01' },
  { key: 'wallet', number: '02' },
  { key: 'notifications', number: '03' },
] as const

const features = [
  { key: 'activity', icon: Bitcoin },
  { key: 'balances', icon: CircleDollarSign },
  { key: 'drain', icon: Shield },
  { key: 'languages', icon: Globe2 },
] as const

const privacyItems = [
  { key: 'walletData', icon: WalletCards },
  { key: 'spendingKeys', icon: KeyRound },
  { key: 'notifications', icon: Webhook },
] as const

const faqKeys = ['operation', 'privateKeys', 'nodes', 'methods', 'privacy', 'cloud'] as const

export default function LandingPage() {
  const t = useTranslations('landing')

  return (
    <div className="min-h-screen overflow-x-hidden">
      <header className="container mx-auto flex h-20 items-center justify-between px-4">
        <Link href="/" className="flex items-center gap-3 transition-opacity hover:opacity-80">
          <Image src="/images/canary.svg" alt="" width={40} height={40} className="h-10 w-10" priority />
          <span className="text-lg font-bold tracking-wide">Canary Wallet</span>
        </Link>

        <nav className="hidden items-center gap-6 text-sm md:flex" aria-label={t('nav.label')}>
          <Link href="#install" className="text-muted-foreground transition-colors hover:text-foreground">{t('nav.install')}</Link>
          <Link href="#how-it-works" className="text-muted-foreground transition-colors hover:text-foreground">{t('nav.howItWorks')}</Link>
          <Link href="#features" className="text-muted-foreground transition-colors hover:text-foreground">{t('nav.features')}</Link>
          <Link href="/cloud" className="text-muted-foreground transition-colors hover:text-foreground">{t('nav.cloud')}</Link>
          <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="text-muted-foreground transition-colors hover:text-foreground">{sourceOption.name}</a>
          <Button variant="outline" size="sm" asChild><Link href="/sign-in">{t('nav.signIn')}</Link></Button>
        </nav>

        <div className="md:hidden">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="icon" aria-label={t('nav.openMenu')}><Menu /></Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-52">
              <DropdownMenuItem asChild><Link href="#install">{t('nav.install')}</Link></DropdownMenuItem>
              <DropdownMenuItem asChild><Link href="#how-it-works">{t('nav.howItWorks')}</Link></DropdownMenuItem>
              <DropdownMenuItem asChild><Link href="#features">{t('nav.features')}</Link></DropdownMenuItem>
              <DropdownMenuItem asChild><Link href="/cloud">{t('nav.cloud')}</Link></DropdownMenuItem>
              <DropdownMenuItem asChild><a href={sourceOption.url} target="_blank" rel="noopener noreferrer">{sourceOption.name}<ExternalLink className="ml-auto" /></a></DropdownMenuItem>
              <DropdownMenuItem asChild><Link href="/sign-in">{t('nav.signIn')}</Link></DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>

      <main>
        <section className="container mx-auto grid items-center gap-10 px-4 pb-14 pt-8 lg:grid-cols-[1.05fr_0.95fr] lg:pb-20 lg:pt-14">
          <div className="max-w-2xl">
            <div className="mb-5 inline-flex items-center gap-2 rounded-full border bg-card px-3 py-1 text-sm text-muted-foreground">
              <Radio className="h-4 w-4 text-primary" />
              {t('hero.eyebrow')}
            </div>
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl lg:text-6xl">{t('hero.title')}</h1>
            <p className="mt-6 max-w-xl text-lg leading-8 text-muted-foreground">{t('hero.description')}</p>
            <div className="mt-8 flex flex-wrap gap-3">
              <Button size="lg" asChild><Link href="#install">{t('hero.install')}<ArrowRight /></Link></Button>
              <Button size="lg" variant="secondary" asChild><Link href="/demo">{t('hero.demo')}</Link></Button>
            </div>
            <Link href="/cloud" className="mt-5 inline-flex max-w-xl items-center gap-2 text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline">
              <Cloud className="h-4 w-4 shrink-0" />
              {t('hero.cloudLink')}
            </Link>
          </div>

          <Card id="install" className="scroll-mt-6 border-primary/20 shadow-md">
            <CardHeader>
              <div className="mb-2 flex h-12 w-12 items-center justify-center rounded-lg bg-muted">
                <Image src="/images/canary.svg" alt="" width={38} height={38} />
              </div>
              <CardTitle className="text-2xl">{t('install.title')}</CardTitle>
              <p className="text-sm leading-6 text-muted-foreground">{t('install.description')}</p>
            </CardHeader>
            <CardContent className="space-y-3">
              {installOptions.map((option) => (
                <a
                  key={option.id}
                  href={option.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-4 rounded-lg border bg-background p-4 transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
                >
                  <Image src={option.logo} alt="" width={34} height={34} className="h-9 w-9" />
                  <span className="min-w-0 flex-1">
                    <span className="block font-medium">{option.name}</span>
                    <span className="block text-sm text-muted-foreground">{t(`install.options.${option.id as InstallOptionId}.description`)}</span>
                  </span>
                  <ExternalLink className="h-4 w-4 text-muted-foreground" />
                </a>
              ))}
              <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-2 pt-2 text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline">
                <Code2 className="h-4 w-4" />{t('install.manual')}
              </a>
            </CardContent>
          </Card>
        </section>

        <section className="border-y bg-muted/30">
          <div className="container mx-auto px-4 py-16">
            <div className="mb-9 max-w-2xl">
              <h2 className="text-3xl font-semibold tracking-tight">{t('trust.title')}</h2>
              <p className="mt-3 text-muted-foreground">{t('trust.description')}</p>
            </div>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              {trustItems.map(({ key, icon: Icon }) => (
                <Card key={key} className="gap-4 py-5 shadow-none">
                  <CardHeader className="px-5"><Icon className="mb-3 h-5 w-5 text-primary" /><CardTitle>{t(`trust.items.${key}.title`)}</CardTitle></CardHeader>
                  <CardContent className="px-5 text-sm leading-6 text-muted-foreground">{t(`trust.items.${key}.description`)}</CardContent>
                </Card>
              ))}
            </div>
          </div>
        </section>

        <section id="how-it-works" className="container mx-auto scroll-mt-6 px-4 py-20">
          <div className="mb-10 max-w-2xl">
            <p className="text-sm font-medium text-primary">{t('how.eyebrow')}</p>
            <h2 className="mt-2 text-3xl font-semibold tracking-tight">{t('how.title')}</h2>
            <p className="mt-3 text-muted-foreground">{t('how.description')}</p>
          </div>
          <div className="grid gap-6 md:grid-cols-3">
            {steps.map(({ key, number }) => (
              <div key={key} className="border-t pt-5">
                <span className="font-mono text-sm text-muted-foreground">{number}</span>
                <h3 className="mt-5 text-lg font-semibold">{t(`how.steps.${key}.title`)}</h3>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">{t(`how.steps.${key}.description`)}</p>
              </div>
            ))}
          </div>
        </section>

        <section id="features" className="scroll-mt-6 border-y bg-muted/30">
          <div className="container mx-auto px-4 py-20">
            <div className="mx-auto mb-10 max-w-2xl text-center">
              <h2 className="text-3xl font-semibold tracking-tight">{t('features.title')}</h2>
              <p className="mt-3 text-muted-foreground">{t('features.description')}</p>
            </div>
            <div className="mx-auto grid max-w-5xl gap-4 md:grid-cols-2">
              {features.map(({ key, icon: Icon }) => (
                <Card key={key} className="gap-4 shadow-none">
                  <CardHeader><Icon className="mb-3 h-5 w-5 text-primary" /><CardTitle>{t(`features.items.${key}.title`)}</CardTitle></CardHeader>
                  <CardContent className="text-sm leading-6 text-muted-foreground">{t(`features.items.${key}.description`)}</CardContent>
                </Card>
              ))}
            </div>
          </div>
        </section>

        <section className="container mx-auto grid gap-10 px-4 py-20 lg:grid-cols-[0.8fr_1.2fr]">
          <div>
            <p className="text-sm font-medium text-primary">{t('privacy.eyebrow')}</p>
            <h2 className="mt-2 text-3xl font-semibold tracking-tight">{t('privacy.title')}</h2>
            <p className="mt-4 leading-7 text-muted-foreground">{t('privacy.description')}</p>
          </div>
          <div className="space-y-4">
            {privacyItems.map(({ key, icon: Icon }) => (
              <Card key={key} className="flex-row items-start gap-4 p-5 shadow-none">
                <Icon className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
                <div><h3 className="font-semibold">{t(`privacy.items.${key}.title`)}</h3><p className="mt-1 text-sm leading-6 text-muted-foreground">{t(`privacy.items.${key}.description`)}</p></div>
              </Card>
            ))}
          </div>
        </section>

        <section className="container mx-auto px-4 pb-20">
          <Card className="mx-auto max-w-5xl border-primary/20 bg-muted/30">
            <CardContent className="grid items-center gap-7 py-2 md:grid-cols-[1fr_auto]">
              <div><div className="mb-3 inline-flex items-center gap-2 text-sm font-medium text-primary"><Cloud className="h-4 w-4" />{t('cloud.eyebrow')}</div><h2 className="text-2xl font-semibold">{t('cloud.title')}</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">{t('cloud.description')}</p></div>
              <Button variant="outline" asChild><Link href="/cloud">{t('cloud.action')}<ArrowRight /></Link></Button>
            </CardContent>
          </Card>
        </section>

        <section id="faq" className="border-y bg-muted/30">
          <div className="container mx-auto px-4 py-20">
            <h2 className="text-center text-3xl font-semibold tracking-tight">{t('faq.title')}</h2>
            <div className="mx-auto mt-10 grid max-w-5xl gap-4 md:grid-cols-2">
              {faqKeys.map((key) => (
                <Card key={key} className="gap-3 py-5 shadow-none">
                  <CardHeader className="px-5"><CardTitle className="leading-6">{t(`faq.items.${key}.question`)}</CardTitle></CardHeader>
                  <CardContent className="px-5 text-sm leading-6 text-muted-foreground">{t(`faq.items.${key}.answer`)}</CardContent>
                </Card>
              ))}
            </div>
          </div>
        </section>

        <section className="container mx-auto px-4 py-20">
          <Card className="mx-auto max-w-4xl bg-primary text-primary-foreground">
            <CardContent className="py-6 text-center">
              <h2 className="text-3xl font-semibold">{t('final.title')}</h2>
              <p className="mx-auto mt-3 max-w-xl opacity-80">{t('final.description')}</p>
              <div className="mt-7 flex flex-wrap justify-center gap-3">
                <Button size="lg" variant="secondary" asChild><Link href="#install">{t('final.install')}<ArrowRight /></Link></Button>
                <Button size="lg" variant="outline" className="border-primary-foreground bg-transparent text-primary-foreground hover:bg-primary-foreground/10 hover:text-primary-foreground" asChild><Link href="/demo">{t('final.demo')}</Link></Button>
                <Button size="lg" variant="outline" className="border-primary-foreground bg-transparent text-primary-foreground hover:bg-primary-foreground/10 hover:text-primary-foreground" asChild><Link href="/donations"><Heart />{t('final.donate')}</Link></Button>
              </div>
            </CardContent>
          </Card>
        </section>
      </main>

      <footer className="border-t">
        <div className="container mx-auto flex flex-col gap-4 px-4 py-8 text-sm text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
          <p>{t('footer.description')}</p>
          <a href={sourceOption.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-2 hover:text-foreground"><Code2 className="h-4 w-4" />{t('footer.source')}</a>
        </div>
      </footer>
    </div>
  )
}
