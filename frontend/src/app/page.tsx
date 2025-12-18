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

  // FOSS mode: show loading while useEffect handles the redirect
  if (isSelfHostedMode) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // SAAS mode: Show landing page for unauthenticated users
  // Note: This handles both explicit navigation to / and post-logout redirects
  if (isCloudMode && !isAuthenticated) {
    return <LandingPage />
  }

  // Authenticated users will be redirected by the useEffect above
  // Return null while redirect is in progress
  return null
}
