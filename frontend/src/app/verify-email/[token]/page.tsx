'use client'

import { useParams, useRouter } from 'next/navigation'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { CheckCircle, XCircle, Loader2 } from 'lucide-react'

export default function VerifyEmailPage() {
  const params = useParams()
  const router = useRouter()
  const [status, setStatus] = useState<'loading' | 'success' | 'error'>('loading')
  const [message, setMessage] = useState('')

  useEffect(() => {
    const verifyEmail = async () => {
      try {
        const response = await fetch(`/api/auth/verify-email/${params.token}`, {
          method: 'GET',
        })

        if (response.ok) {
          setStatus('success')
          setMessage('Your email has been verified successfully! You can now log in.')
        } else {
          const errorData = await response.json()
          setStatus('error')
          setMessage(errorData.error || 'Email verification failed')
        }
      } catch {
        setStatus('error')
        setMessage('Network error occurred during verification')
      }
    }

    if (params.token) {
      verifyEmail()
    }
  }, [params.token])

  const handleContinue = () => {
    router.push('/sign-in')
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 py-12 px-4 sm:px-6 lg:px-8">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle>Email Verification</CardTitle>
          <CardDescription>
            {status === 'loading' && 'Verifying your email address...'}
            {status === 'success' && 'Verification Complete'}
            {status === 'error' && 'Verification Failed'}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex justify-center">
            {status === 'loading' && (
              <Loader2 className="h-12 w-12 text-blue-500 animate-spin" />
            )}
            {status === 'success' && (
              <CheckCircle className="h-12 w-12 text-green-500" />
            )}
            {status === 'error' && (
              <XCircle className="h-12 w-12 text-red-500" />
            )}
          </div>

          <p className="text-center text-sm text-gray-600">
            {message}
          </p>

          {status !== 'loading' && (
            <Button 
              onClick={handleContinue} 
              className="w-full"
              variant={status === 'success' ? 'default' : 'outline'}
            >
              {status === 'success' ? 'Continue to Login' : 'Back to Login'}
            </Button>
          )}
        </CardContent>
      </Card>
    </div>
  )
}