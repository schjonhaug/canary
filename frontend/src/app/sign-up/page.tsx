'use client'

import { useState } from 'react'
import { notFound, useRouter } from 'next/navigation'
import { useAuth } from '@/contexts/auth-context'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorDisplay } from '@/components/ui/error-display'
import { Loader2 } from 'lucide-react'
import Image from 'next/image'
import Link from 'next/link'
import { useTranslations } from 'next-intl'
import { ApiError, getTranslatedApiError } from '@/lib/utils'

export default function SignUpPage() {
  const t = useTranslations('auth.signUp')
  const tCommon = useTranslations('common')
  const tErrors = useTranslations('errors.api')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [name, setName] = useState('')
  const [marketingEmails, setMarketingEmails] = useState(false)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const { register, isSelfHostedMode } = useAuth()
  const router = useRouter()

  if (isSelfHostedMode) {
    notFound()
  }
  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      await register(email, password, name, marketingEmails)
      // Redirect to success page immediately
      router.push('/sign-up/success')
    } catch (err) {
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tErrors))
      } else {
        setError(err instanceof Error ? err.message : t('registrationFailed'))
      }
    } finally {
      setIsLoading(false)
    }
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

          <form onSubmit={handleRegister} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">{t('nameLabel')}</Label>
              <Input
                id="name"
                type="text"
                placeholder={t('namePlaceholder')}
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
                disabled={isLoading}
              />
            </div>
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
            <div className="space-y-2">
              <Label htmlFor="password">{tCommon('passwordLabel')}</Label>
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
              <p className="text-sm text-gray-500">{t('passwordHint')}</p>
            </div>
            <div className="flex items-start space-x-2">
              <input
                id="marketing"
                type="checkbox"
                checked={marketingEmails}
                onChange={(e) => setMarketingEmails(e.target.checked)}
                disabled={isLoading}
                className="mt-1 h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
              />
              <Label
                htmlFor="marketing"
                className="text-sm text-gray-600 cursor-pointer select-none"
              >
                {t('marketingConsent')}
              </Label>
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={isLoading || !email || !password || !name}
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
                {t('hasAccount')}
              </Button>
            </Link>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
