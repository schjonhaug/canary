'use client'

import { useEffect } from 'react'
import { useAuth } from '@/contexts/auth-context'

export default function SignOutPage() {
  const { logout } = useAuth()

  useEffect(() => {
    const performSignOut = async () => {
      try {
        await logout()
        // Use hard navigation to ensure clean state after logout
        window.location.href = '/'
      } catch (error) {
        console.error('Sign out error:', error)
        window.location.href = '/'
      }
    }

    performSignOut()
  }, [logout])

  return null
}