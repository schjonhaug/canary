'use client'

import { useParams, useRouter } from 'next/navigation'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { CheckCircle, XCircle, Loader2 } from 'lucide-react'
import { useTranslations } from 'next-intl'

export default function VerifyEmailPage() {
  const t = useTranslations('auth.verifyEmail')
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
          setMessage(t('successMessage'))
        } else {
          const errorData = await response.json()
          setStatus('error')
          setMessage(errorData.error || t('invalidToken'))
        }
      } catch {
        setStatus('error')
        setMessage(t('invalidToken'))
      }
    }

    if (params.token) {
      verifyEmail()
    }
  }, [params.token, t])

  const handleContinue = () => {
    router.push('/sign-in')
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 py-12 px-4 sm:px-6 lg:px-8">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle>
            {status === 'success' ? t('successTitle') : status === 'error' ? t('errorTitle') : t('verifying')}
          </CardTitle>
          <CardDescription>
            {status === 'loading' && t('verifying')}
            {status === 'success' && t('successTitle')}
            {status === 'error' && t('errorTitle')}
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
              {t('signIn')}
            </Button>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
