'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import LandingPage from '@/components/landing-page'

export default function HomePage() {
  const { isAuthenticated, isLoading, isSaasMode, isFossMode } = useAuth()
  const router = useRouter()

  useEffect(() => {
    // If user is authenticated or in FOSS mode, redirect to wallets
    if (!isLoading && (isAuthenticated || isFossMode)) {
      router.push('/wallets')
    }
  }, [isAuthenticated, isLoading, isFossMode, router])

  // Show loading while checking auth
  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mx-auto"></div>
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // FOSS mode: redirect directly to wallets (no landing page)
  if (isFossMode) {
    router.push('/wallets')
    return null
  }

  // SAAS mode: Show landing page for unauthenticated users
  if (isSaasMode && !isAuthenticated) {
    return <LandingPage />
  }

  // This should not be reached due to the useEffect redirect above
  return null
}