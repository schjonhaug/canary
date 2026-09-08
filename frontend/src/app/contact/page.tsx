'use client'

import { useTranslations } from 'next-intl'
import { notFound } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { ContactForm } from '@/components/contact-form'

export default function ContactPage() {
  const t = useTranslations('contactPage')
  const { isSelfHostedMode } = useAuth()
  if (isSelfHostedMode) notFound()
  return <div className="space-y-6"><h2 className="text-2xl font-semibold">{t('title')}</h2><div className="max-w-md"><ContactForm /></div></div>
}
