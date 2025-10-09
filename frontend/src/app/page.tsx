'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import LandingPage from '@/components/landing-page'
import { LoadingSpinner } from '@/components/ui/loading-spinner'

export default function HomePage() {
  const { isAuthenticated, isLoading, isCloudMode, isSelfHostedMode } = useAuth()
  const router = useRouter()

  useEffect(() => {
    // If user is authenticated or in FOSS mode, redirect to wallets
    if (!isLoading && (isAuthenticated || isSelfHostedMode)) {
      router.push('/wallets')
    }
  }, [isAuthenticated, isLoading, isSelfHostedMode, router])

  // Show loading while checking auth
  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // FOSS mode: redirect directly to wallets (no landing page)
  if (isSelfHostedMode) {
    router.push('/wallets')
    return null
  }

  // SAAS mode: Show landing page for unauthenticated users
  if (isCloudMode && !isAuthenticated) {
    return <LandingPage />
  }

  // This should not be reached due to the useEffect redirect above
  return null
}
