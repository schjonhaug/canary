'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import Image from 'next/image'
import Link from 'next/link'
import { Button } from '@/components/ui/button'

export default function HomePage() {
  const { isAuthenticated, isLoading } = useAuth()
  const router = useRouter()
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED === 'true'

  useEffect(() => {
    // If auth is disabled (FOSS mode) or user is authenticated, redirect to wallets
    if (!isLoading && (!authEnabled || isAuthenticated)) {
      router.push('/wallets')
    }
  }, [isAuthenticated, isLoading, authEnabled, router])

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

  // Show landing page for unauthenticated users when auth is enabled
  if (authEnabled && !isAuthenticated) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="text-center">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={120}
            height={120}
            className="mx-auto mb-6"
          />
          <h1 className="text-4xl font-bold tracking-wide">Canary</h1>
        </div>
      </div>
    )
  }

  // This should not be reached due to the useEffect redirect above
  return null
}