'use client'

import Link from 'next/link'
import { useTranslations } from 'next-intl'
import { ArrowRight } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { privateOffer } from '@/lib/private-offer'

export function PrivatePromotion() {
  const t = useTranslations('privatePage')
  return (
    <section aria-label={t('name')} className="container mx-auto px-4 py-12">
      <Card className="mx-auto max-w-5xl shadow-none">
        <CardContent className="flex flex-col gap-6 py-6 md:flex-row md:items-center md:justify-between">
          <div className="max-w-2xl">
            <p className="mb-3 text-sm font-medium text-primary">{t('name')}</p>
            <h2 className="text-2xl font-semibold">{t('tagline')}</h2>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">{t('description')}</p>
            <p className="mt-4 text-lg font-semibold">{t('price', { price: privateOffer.startingPrice })}</p>
            <p className="mt-1 text-sm text-muted-foreground">{t('terms')}</p>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">{t('availability')}</p>
          </div>
          <Button size="lg" asChild><Link href="/private">{t('learnMore')}<ArrowRight /></Link></Button>
        </CardContent>
      </Card>
    </section>
  )
}

export function PrivateLink() {
  const t = useTranslations('privatePage')
  return <Link href="/private" className="text-sm text-muted-foreground transition-colors hover:text-foreground">{t('nav')}</Link>
}
