'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'

export default function SignOutPage() {
  const { logout, isSelfHostedMode } = useAuth()
  const router = useRouter()

  useEffect(() => {
    const performSignOut = async () => {
      const redirectTo = isSelfHostedMode ? '/sign-in' : '/'

      try {
        await logout()
      } catch (error) {
        console.error('Sign out error:', error)
      } finally {
        router.push(redirectTo)
      }
    }

    performSignOut()
  }, [logout, isSelfHostedMode, router])

  return null
}
