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
  }, [user?.is_demo])

  const handleDismiss = () => {
    setIsVisible(false)
    localStorage.setItem('demo_banner_dismissed', 'true')
  }

  if (!isVisible || !user?.is_demo) {
    return null
  }

  return (
    <div className="bg-blue-600 text-white">
      <div className="container mx-auto px-4 py-3">
        <div className="flex items-center justify-between">
          <div className="flex-1 flex items-center gap-3">
            <span className="font-semibold">{t('title')}</span>
            <span className="text-blue-100">
              {t('description')}
            </span>
            <Link
              href="/sign-up"
              className="ml-4 px-4 py-1.5 bg-white text-blue-600 hover:bg-blue-50 rounded-md text-sm font-medium transition-colors"
            >
              {t('signUp')}
            </Link>
          </div>
          <button
            onClick={handleDismiss}
            className="ml-4 p-1 hover:bg-blue-700 rounded transition-colors"
            aria-label={t('dismiss')}
          >
            <X className="h-5 w-5" />
          </button>
        </div>
      </div>
    </div>
  )
}
