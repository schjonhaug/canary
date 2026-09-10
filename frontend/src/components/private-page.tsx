'use client'

import Image from 'next/image'
import Link from 'next/link'
import { useTranslations } from 'next-intl'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ContactForm } from '@/components/contact-form'
import { privateOffer } from '@/lib/private-offer'

export default function PrivatePageContent() {
  const t = useTranslations('privatePage')
  const cloud = useTranslations('cloudPage')
  return (
    <div className="min-h-screen overflow-x-hidden">
      <header className="container mx-auto flex min-h-20 flex-wrap items-center justify-between gap-4 px-4 py-4">
        <Link href="/" className="flex items-center gap-3 hover:opacity-80">
          <Image src="/images/canary.svg" alt="" width={40} height={40} priority />
          <span className="font-bold tracking-wide">Canary Wallet</span>
        </Link>
        <nav aria-label={cloud('nav.label')} className="flex items-center gap-4">
          <Link href="/cloud" className="text-sm text-muted-foreground hover:text-foreground">Canary Cloud</Link>
          <Button variant="outline" size="sm" asChild><a href="#enquiry">{t('contact')}</a></Button>
        </nav>
      </header>
      <main>
        <section className="container mx-auto px-4 py-16 text-center sm:py-20">
          <div className="mx-auto max-w-3xl">
            <p className="mb-5 font-medium text-primary">{t('name')}</p>
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">{t('tagline')}</h1>
            <p className="mt-6 text-lg leading-8 text-muted-foreground">{t('description')}</p>
            <p className="mt-8 text-3xl font-semibold">{t('price', { price: privateOffer.startingPrice })}</p>
            <p className="mt-3 text-sm text-muted-foreground">{t('terms')}</p>
            <p className="mx-auto mt-5 max-w-2xl text-sm leading-6 text-muted-foreground">{t('availability')}</p>
            <Button size="lg" className="mt-7" asChild><a href="#enquiry">{t('contact')}</a></Button>
          </div>
        </section>
        <section className="border-y bg-muted/30">
          <div className="container mx-auto max-w-5xl px-4 py-16">
            <h2 className="text-center text-3xl font-semibold">{t('benefitsTitle')}</h2>
            <p className="mx-auto mt-4 max-w-2xl text-center text-muted-foreground">{t('audience')}</p>
            <div className="mt-10 grid gap-4 md:grid-cols-2">
              {(['monitoring', 'control', 'service', 'watchOnly'] as const).map(key => (
                <Card key={key} className="gap-3 shadow-none">
                  <CardHeader><CardTitle>{t(`benefits.${key}.title`)}</CardTitle></CardHeader>
                  <CardContent className="text-sm leading-6 text-muted-foreground">{t(`benefits.${key}.description`)}</CardContent>
                </Card>
              ))}
            </div>
          </div>
        </section>
        <section className="container mx-auto max-w-5xl px-4 py-16">
          <h2 className="text-center text-3xl font-semibold">{t('stepsTitle')}</h2>
          <ol className="mt-10 grid gap-8 md:grid-cols-3">
            {(['needs', 'quote', 'setup'] as const).map((key, index) => (
              <li key={key}>
                <span aria-hidden="true" className="text-3xl font-semibold text-primary">{index + 1}</span>
                <h3 className="mt-3 text-lg font-semibold">{t(`steps.${key}.title`)}</h3>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">{t(`steps.${key}.description`)}</p>
              </li>
            ))}
          </ol>
        </section>
        <section className="container mx-auto max-w-5xl px-4 py-12">
          <h2 className="text-center text-3xl font-semibold">{t('faqTitle')}</h2>
          <div className="mt-10 grid gap-4 md:grid-cols-2">
            {(['cloud', 'privacy', 'maintenance', 'pricing'] as const).map(key => (
              <Card key={key} className="gap-3 shadow-none">
                <CardHeader><CardTitle className="leading-6">{t(`faq.${key}.question`)}</CardTitle></CardHeader>
                <CardContent className="text-sm leading-6 text-muted-foreground">{t(`faq.${key}.answer`, { price: privateOffer.startingPrice })}</CardContent>
              </Card>
            ))}
          </div>
        </section>
        <section id="enquiry" aria-labelledby="enquiry-title" tabIndex={-1} className="container mx-auto scroll-mt-8 px-4 py-16">
          <h2 id="enquiry-title" className="mb-8 text-center text-3xl font-semibold">{t('contact')}</h2>
          <div className="mx-auto max-w-xl">
            <ContactForm messagePrefix={privateOffer.enquiryPrefix} title={t('form.title')} description={t('form.description')} placeholder={t('form.placeholder')} successMessage={t('form.success')} />
          </div>
        </section>
      </main>
      <footer className="border-t">
        <div className="container mx-auto flex flex-wrap justify-between gap-4 px-4 py-8 text-sm text-muted-foreground">
          <Link href="/#install" className="hover:text-foreground">{cloud('footer.selfHost')}</Link>
          <Link href="/cloud" className="hover:text-foreground">Canary Cloud</Link>
        </div>
      </footer>
    </div>
  )
}
