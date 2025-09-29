'use client'

import { useEffect, useState } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Loader2 } from 'lucide-react'

export default function SignOutPage() {
  const router = useRouter()
  const { logout } = useAuth()
  const [isSigningOut, setIsSigningOut] = useState(true)

  useEffect(() => {
    const performSignOut = async () => {
      try {
        await logout()
        // Redirect to home page after logout
        router.push('/')
      } catch (error) {
        console.error('Sign out error:', error)
        // Even if there's an error, redirect to home
        router.push('/')
      } finally {
        setIsSigningOut(false)
      }
    }

    performSignOut()
  }, [logout, router])

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 py-12 px-4 sm:px-6 lg:px-8">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle>Signing Out</CardTitle>
          <CardDescription>Please wait while we sign you out...</CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex justify-center">
            {isSigningOut && (
              <Loader2 className="h-12 w-12 text-blue-500 animate-spin" />
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}