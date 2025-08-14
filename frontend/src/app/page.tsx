'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import LandingPage from '@/components/landing-page'

export default function HomePage() {
  const { isAuthenticated, isLoading } = useAuth()
  const router = useRouter()
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED === 'true'

  useEffect(() => {
    // If user is authenticated, redirect to wallets
    if (!isLoading && isAuthenticated) {
      router.push('/wallets')
    }
  }, [isAuthenticated, isLoading, router])

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

  // When auth is disabled, redirect to wallets
  if (!authEnabled) {
    router.push('/wallets')
    return null
  }

  // Show landing page for unauthenticated users when auth is enabled
  if (authEnabled && !isAuthenticated) {
    return <LandingPage />
  }

  // This should not be reached due to the useEffect redirect above
  return null
}