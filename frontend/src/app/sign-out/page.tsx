'use client'

import { useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'

export default function SignOutPage() {
  const router = useRouter()
  const { logout } = useAuth()

  useEffect(() => {
    const performSignOut = async () => {
      try {
        await logout()
        router.push('/')
      } catch (error) {
        console.error('Sign out error:', error)
        router.push('/')
      }
    }

    performSignOut()
  }, [logout, router])

  return null
}