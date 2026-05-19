'use client'

import { useEffect, useState } from 'react'
import { useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorDisplay } from '@/components/ui/error-display'
import { Loader2, User, Shield } from 'lucide-react'
import Image from 'next/image'
import Link from 'next/link'
import { useTranslations } from 'next-intl'
import { ApiError, getTranslatedApiError } from '@/lib/utils'
import { SELF_HOSTED_ADMIN_EMAIL } from '@/lib/constants'

export default function SignInPage() {
  const t = useTranslations('auth.signIn')
  const tCommon = useTranslations('common')
  const tErrors = useTranslations('errors.api')
  const { login, isAuthenticated, isSelfHostedMode } = useAuth()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const router = useRouter()

  // Redirect authenticated users to wallets
  useEffect(() => {
    if (isAuthenticated) {
      router.push('/wallets')
    }
  }, [isAuthenticated, router])

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      await login(isSelfHostedMode ? SELF_HOSTED_ADMIN_EMAIL : email, password)
      // Navigation is handled by the login function in auth context
    } catch (err) {
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tErrors))
      } else {
        setError(err instanceof Error ? err.message : t('loginFailed'))
      }
    } finally {
      setIsLoading(false)
    }
  }

  const handleDevLogin = async (devEmail: string) => {
    setError('')
    setIsLoading(true)

    try {
      await login(devEmail, 'password123')
      // Navigation is handled by the login function in auth context
    } catch (err) {
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tErrors))
      } else {
        setError(err instanceof Error ? err.message : 'Failed to login')
      }
    } finally {
      setIsLoading(false)
    }
  }
  // Don't render anything while redirecting authenticated users
  if (isAuthenticated) {
    return null
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

          {/* Development Mode Quick Login */}
          {process.env.NODE_ENV === 'development' && (
            <div className="mb-6 p-4 bg-blue-50 border border-blue-200 rounded-lg">
              <div className="flex items-center gap-2 mb-3">
                <Shield className="h-4 w-4 text-blue-600" />
                <span className="text-sm font-medium text-blue-800">{t('devMode')}</span>
              </div>
              <div className="space-y-2">
                <div className="grid grid-cols-1 gap-2">
                  {[
                    { email: 'delivered+admin@resend.dev', label: 'Admin' },
                    { email: 'delivered+alice@resend.dev', label: 'Alice (Personal)' },
                    { email: 'delivered+bob@resend.dev', label: 'Bob (Team)' },
                    { email: 'delivered+charlie@resend.dev', label: 'Charlie (Team)' }
                  ].map(({ email: devEmail, label }) => (
                    <Button
                      key={devEmail}
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => handleDevLogin(devEmail)}
                      disabled={isLoading}
                    >
                      <User className="mr-2 h-3 w-3" />
                      {label}
                    </Button>
                  ))}
                </div>
              </div>
            </div>
          )}

          <form onSubmit={handleLogin} className="space-y-4">
            {isSelfHostedMode ? (
              <input
                type="text"
                name="username"
                value={SELF_HOSTED_ADMIN_EMAIL}
                autoComplete="username"
                readOnly
                className="hidden"
                tabIndex={-1}
              />
            ) : (
              <div className="space-y-2">
                <Label htmlFor="email">{tCommon('emailLabel')}</Label>
                <Input
                  id="email"
                  name="username"
                  type="email"
                  placeholder={tCommon('emailPlaceholder')}
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  autoComplete="username"
                  required
                  disabled={isLoading}
                />
              </div>
            )}
            <div className="space-y-2">
              <Label htmlFor="password">{tCommon('passwordLabel')}</Label>
              <Input
                id="password"
                name="password"
                type="password"
                placeholder={t('passwordPlaceholder')}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="current-password"
                required
                disabled={isLoading}
              />
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={isLoading || (!isSelfHostedMode && !email) || !password}
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
            {!isSelfHostedMode && (
              <div className="flex flex-col gap-2">
                <Link href="/sign-up">
                  <Button
                    type="button"
                    variant="outline"
                    className="w-full"
                    disabled={isLoading}
                  >
                    {t('noAccount')}
                  </Button>
                </Link>
                <Link href="/forgot-password">
                  <Button
                    type="button"
                    variant="outline"
                    className="w-full"
                    disabled={isLoading}
                  >
                    {t('forgotPassword')}
                  </Button>
                </Link>
              </div>
            )}
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
