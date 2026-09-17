'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import LandingPage from '@/components/landing-page'
import { LoadingSpinner } from '@/components/ui/loading-spinner'
import { useTranslations } from 'next-intl'

export default function HomePage() {
  const { isAuthenticated, isLoading, isCloudMode, isSelfHostedMode, user } = useAuth()
  const router = useRouter()
  const tCommon = useTranslations('common')

  useEffect(() => {
    if (isLoading) {
      return
    }

    if (isAuthenticated) {
      router.push(isCloudMode && user?.is_admin ? '/support' : '/wallets')
      return
    }

    if (isSelfHostedMode) {
      router.push('/sign-in')
    }
  }, [isAuthenticated, isLoading, isSelfHostedMode, isCloudMode, user?.is_admin, router])

  // Cloud mode: render the marketing page for signed-out visitors, including
  // during the session probe, so `/` SSRs the landing page instead of a spinner.
  if (isCloudMode && !isAuthenticated) {
    return <LandingPage />
  }

  // Show loading while checking auth, or while self-hosted redirects to sign-in
  if (isLoading || isSelfHostedMode) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon('loading')}</p>
        </div>
      </div>
    )
  }

  // Authenticated users will be redirected by the useEffect above
  // Return null while redirect is in progress
  return null
}
