'use client'

import { useState } from 'react'
import { notFound, useRouter, useParams } from 'next/navigation'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorDisplay, SuccessDisplay } from '@/components/ui/error-display'
import { Loader2 } from 'lucide-react'
import Image from 'next/image'
import Link from 'next/link'
import { api, ApiError } from '@/lib/api'
import { getTranslatedApiError } from '@/lib/utils'
import { useTranslations } from 'next-intl'
import { useAuth } from '@/contexts/auth-context'

export default function ResetPasswordPage() {
  const t = useTranslations('auth.resetPassword')
  const tSignUp = useTranslations('auth.signUp')
  const tApiErrors = useTranslations('errors.api')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState(false)
  const router = useRouter()
  const params = useParams()
  const { isSelfHostedMode } = useAuth()
  const token = params.token as string

  if (isSelfHostedMode) {
    notFound()
  }

  const handleResetPassword = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')

    // Validate passwords match
    if (password !== confirmPassword) {
      setError(t('passwordMismatch'))
      return
    }

    if (password.length < 6) {
      setError(tSignUp('passwordHint'))
      return
    }

    setIsLoading(true)

    try {
      await api.resetPassword(token, password)
      setSuccess(true)
      // Redirect to sign-in after a short delay
      setTimeout(() => {
        router.push('/sign-in')
      }, 3000)
    } catch (err) {
      if (err instanceof ApiError || err instanceof Error) {
        setError(getTranslatedApiError(err, tApiErrors))
      } else {
        setError(t('invalidToken'))
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
                alt="Canary Wallet Logo"
                width={48}
                height={48}
                className="h-12 w-12"
              />
            </div>
            <CardTitle className="text-2xl font-bold text-center">
              {t('successTitle')}
            </CardTitle>
            <CardDescription className="text-center">
              {t('successMessage')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <SuccessDisplay message={t('successMessage')} className="mb-4" />

            <Link href="/sign-in" className="block">
              <Button className="w-full">
                {t('submit')}
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
              alt="Canary Wallet Logo"
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

          <form onSubmit={handleResetPassword} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="password">{t('passwordLabel')}</Label>
              <Input
                id="password"
                type="password"
                placeholder={t('passwordPlaceholder')}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                disabled={isLoading}
                minLength={6}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="confirmPassword">{t('confirmLabel')}</Label>
              <Input
                id="confirmPassword"
                type="password"
                placeholder={t('confirmPlaceholder')}
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                required
                disabled={isLoading}
                minLength={6}
              />
              <p className="text-sm text-gray-500">{tSignUp('passwordHint')}</p>
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={isLoading || !password || !confirmPassword}
            >
              {isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {t('submitting')}
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
                {t('submit')}
              </Button>
            </Link>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
