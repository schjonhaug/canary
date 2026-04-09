'use client'

import { useState } from 'react'
import { notFound } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { api, ApiError } from '@/lib/api'
import { getTranslatedApiError } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorDisplay, SuccessDisplay } from '@/components/ui/error-display'
import { Loader2 } from 'lucide-react'
import Image from 'next/image'
import Link from 'next/link'
import { useTranslations } from 'next-intl'

export default function ForgotPasswordPage() {
  const t = useTranslations('auth.forgotPassword')
  const tCommon = useTranslations('common')
  const tApiErrors = useTranslations('errors.api')
  const [email, setEmail] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState(false)
  const { isSelfHostedMode } = useAuth()

  if (isSelfHostedMode) {
    notFound()
  }
  const handleForgotPassword = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setSuccess(false)
    setIsLoading(true)

    try {
      await api.forgotPassword(email)
      setSuccess(true)
      setEmail('') // Clear the form
    } catch (err) {
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tApiErrors))
      } else {
        setError(err instanceof Error ? err.message : 'Failed to send reset email')
      }
    } finally {
      setIsLoading(false)
    }
  }
  if (success) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50 px-4">
        <Card className="w-full max-w-md">
          <CardHeader className="space-y-1">
            <div className="flex items-center justify-center mb-4">
              <Image
                src="/images/canary.svg"
                alt="Canary Logo"
                width={48}
                height={48}
                className="h-12 w-12"
              />
            </div>
            <CardTitle className="text-2xl font-bold text-center">
              {t('successTitle')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <SuccessDisplay message={t('successMessage')} className="mb-4" />
            <Link href="/sign-in">
              <Button className="w-full">
                {tCommon('backToSignIn')}
              </Button>
            </Link>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50 px-4">
      <Card className="w-full max-w-md">
        <CardHeader className="space-y-1">
          <div className="flex items-center justify-center mb-4">
            <Image
              src="/images/canary.svg"
              alt="Canary Logo"
              width={48}
              height={48}
              className="h-12 w-12"
            />
          </div>
          <CardTitle className="text-2xl font-bold text-center">
            {t('title')}
          </CardTitle>
          <CardDescription className="text-center">
            {t('subtitle')}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {error && (
            <ErrorDisplay message={error} variant="inline" className="mb-4" />
          )}

          <form onSubmit={handleForgotPassword} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="email">{tCommon('emailLabel')}</Label>
              <Input
                id="email"
                type="email"
                placeholder={tCommon('emailPlaceholder')}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                disabled={isLoading}
              />
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={isLoading || !email}
            >
              {isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {tCommon('sending')}
                </>
              ) : (
                t('submit')
              )}
            </Button>
            <Link href="/sign-in">
              <Button
                type="button"
                variant="outline"
                className="w-full"
                disabled={isLoading}
              >
                {tCommon('backToSignIn')}
              </Button>
            </Link>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
