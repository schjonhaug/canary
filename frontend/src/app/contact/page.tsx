'use client'

import { useState, useEffect } from 'react'
import { useTranslations } from 'next-intl'
import { useAuth } from '@/contexts/auth-context'
import { api, ApiError } from '@/lib/api'
import { EMAIL_REGEX, EMAIL_CONSTRAINTS, MESSAGE_CONSTRAINTS } from '@/lib/constants'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { SuccessDisplay } from '@/components/ui/error-display'
import { Loader2 } from 'lucide-react'
import Link from 'next/link'

export default function ContactPage() {
  const t = useTranslations('contactPage')
  const tCommon = useTranslations('common')
  const { user, isAuthenticated, isSelfHostedMode } = useAuth()
  const [email, setEmail] = useState('')
  const [message, setMessage] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState('')

  // Pre-fill email for logged-in users
  useEffect(() => {
    if (isAuthenticated && user?.email) {
      setEmail(user.email)
    }
  }, [isAuthenticated, user])

  // Validation (uses centralized patterns from constants.ts)
  const validateEmail = (email: string): string | null => {
    if (!email.trim()) {
      return t('validation.emailRequired')
    }
    if (!EMAIL_REGEX.test(email)) {
      return t('validation.emailInvalid')
    }
    if (email.length > EMAIL_CONSTRAINTS.MAX_LENGTH) {
      return t('validation.emailTooLong', { max: EMAIL_CONSTRAINTS.MAX_LENGTH })
    }
    return null
  }

  const validateMessage = (message: string): string | null => {
    if (!message.trim()) {
      return t('validation.messageRequired')
    }
    if (message.trim().length < MESSAGE_CONSTRAINTS.MIN_LENGTH) {
      return t('validation.messageTooShort', { min: MESSAGE_CONSTRAINTS.MIN_LENGTH })
    }
    if (message.length > MESSAGE_CONSTRAINTS.MAX_LENGTH) {
      return t('validation.messageTooLong', { max: MESSAGE_CONSTRAINTS.MAX_LENGTH })
    }
    return null
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setSuccess('')

    // Validate
    const emailError = validateEmail(email)
    if (emailError) {
      setError(emailError)
      return
    }

    const messageError = validateMessage(message)
    if (messageError) {
      setError(messageError)
      return
    }

    setIsLoading(true)

    try {
      const response = await api.submitContactForm(email.trim(), message.trim())
      setSuccess(response.message)
      setMessage('') // Clear message on success, keep email
    } catch (err) {
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors, actual message for validation
        setError(err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message)
      } else {
        setError(err instanceof Error ? err.message : t('errors.sendFailed'))
      }
    } finally {
      setIsLoading(false)
    }
  }

  // In self-hosted mode, show a simple message (no contact form needed)
  if (isSelfHostedMode) {
    return (
      <div className="space-y-6">
        <h2 className="text-2xl font-semibold">{t('title')}</h2>
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>{tCommon('selfHostedMode')}</CardTitle>
            <CardDescription>
              {t('selfHosted.description')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Link href="/wallets">
              <Button variant="outline" className="w-full">
                {tCommon('backToWallets')}
              </Button>
            </Link>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Page Title */}
      <h2 className="text-2xl font-semibold">{t('title')}</h2>

      <Card className="max-w-md">
        <CardHeader className="space-y-1">
          <CardTitle className="text-xl">
            {t('form.title')}
          </CardTitle>
          <CardDescription>
            {t('form.description')}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {error && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          {success && (
            <SuccessDisplay message={success} className="mb-4" />
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
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
              <Label htmlFor="message">{t('form.messageLabel')}</Label>
              <Textarea
                id="message"
                placeholder={t('form.messagePlaceholder')}
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                required
                disabled={isLoading}
                className="min-h-[120px]"
              />
              <p className="text-xs text-muted-foreground">
                {t('form.characterCount', { count: message.length, max: MESSAGE_CONSTRAINTS.MAX_LENGTH })}
              </p>
            </div>
            <Button
              type="submit"
              className="w-full"
              disabled={isLoading || !email || !message}
            >
              {isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  {tCommon('sending')}
                </>
              ) : (
                t('form.submit')
              )}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
