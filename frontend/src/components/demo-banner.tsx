'use client'

import { useEffect, useState } from 'react'
import { useTranslations } from 'next-intl'
import { useAuth } from '@/contexts/auth-context'
import { X } from 'lucide-react'
import Link from 'next/link'

export function DemoBanner() {
  const t = useTranslations('demoBanner')
  const { user } = useAuth()
  const [isVisible, setIsVisible] = useState(false)

  useEffect(() => {
    // Check if banner was previously dismissed
    const dismissed = localStorage.getItem('demo_banner_dismissed')
    if (user?.is_demo && !dismissed) {
      setIsVisible(true)
    } else {
      setIsVisible(false)
    }
  }, [user?.id, user?.is_demo])

  const handleDismiss = () => {
    setIsVisible(false)
    localStorage.setItem('demo_banner_dismissed', 'true')
  }

  if (!isVisible || !user?.is_demo) {
    return null
  }

  return (
    <div className="mb-4 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-amber-950 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-3 sm:flex-row sm:items-center">
          <div className="min-w-0 space-y-1 sm:flex-1">
            <p className="text-sm font-semibold">{t('title')}</p>
            <p className="text-sm leading-5 text-amber-900">
              {t('description')}
            </p>
          </div>
          <Link
            href="/sign-up"
            className="inline-flex w-fit shrink-0 items-center rounded-md border border-amber-300 bg-white px-3 py-1.5 text-sm font-medium text-amber-950 transition-colors hover:bg-amber-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-500 focus-visible:ring-offset-2"
          >
            {t('signUp')}
          </Link>
        </div>
        <button
          onClick={handleDismiss}
          className="shrink-0 rounded-md p-1 text-amber-900 transition-colors hover:bg-amber-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-500 focus-visible:ring-offset-2"
          aria-label={t('dismiss')}
        >
          <X className="h-5 w-5" />
        </button>
      </div>
    </div>
  )
}
