'use client'

import { useEffect } from 'react'
import { useAuth } from '@/contexts/auth-context'
import { useRouter } from 'next/navigation'

export default function DemoPage() {
  const { demoLogin, logout, isAuthenticated, user } = useAuth()
  const router = useRouter()

  useEffect(() => {
    const performDemoLogin = async () => {
      // If already logged in as demo user, redirect to home
      if (isAuthenticated && user?.is_demo) {
        router.push('/')
        return
      }

      // If logged in as different user, log them out first
      if (isAuthenticated && !user?.is_demo) {
        try {
          await logout()
        } catch (error) {
          console.error('Logout failed:', error)
        }
      }

      // Attempt demo login
      try {
        await demoLogin()
        // demoLogin already handles navigation to /
      } catch (error) {
        console.error('Demo login failed:', error)
        // Show error message instead of silent redirect
        router.push('/?error=demo_login_failed')
      }
    }

    performDemoLogin()
  }, [demoLogin, logout, isAuthenticated, user, router])

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="text-center">
        <h1 className="text-2xl font-semibold mb-4">Loading Demo...</h1>
        <p className="text-muted-foreground">You&apos;re being logged into the demo account</p>
      </div>
    </div>
  )
}
