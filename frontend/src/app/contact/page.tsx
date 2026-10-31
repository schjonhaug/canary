'use client'

import { useTranslations } from 'next-intl'
import { notFound } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { ContactForm } from '@/components/contact-form'
import { sourceOption } from '@/lib/install-options'

export default function ContactPage() {
  const t = useTranslations('contactPage')
  const { isSelfHostedMode } = useAuth()
  if (isSelfHostedMode) notFound()
  return (
    <div className="space-y-6">
      <h2 className="text-2xl font-semibold">{t('title')}</h2>
      <div className="max-w-2xl space-y-3 leading-7 text-muted-foreground">
        <p>{t('intro.what')}</p>
        <p>{t('intro.reply')}</p>
        <p>
          {t('intro.github')}{' '}
          <a href={`${sourceOption.url}/issues`} target="_blank" rel="noopener noreferrer" className="underline underline-offset-4 hover:text-foreground">
            {t('intro.githubLink')}
          </a>
        </p>
      </div>
      <div className="max-w-md">
        <ContactForm />
      </div>
    </div>
  )
}
