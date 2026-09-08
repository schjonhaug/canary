'use client'

import { useState, useEffect, useRef } from 'react'
import { useTranslations } from 'next-intl'
import { useAuth } from '@/contexts/auth-context'
import { api, ApiError } from '@/lib/api'
import { EMAIL_CONSTRAINTS, MESSAGE_CONSTRAINTS, isValidEmail } from '@/lib/constants'
import { getTranslatedApiError } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ErrorDisplay, SuccessDisplay } from '@/components/ui/error-display'
import { Loader2 } from 'lucide-react'

export function ContactForm({ messagePrefix = '', title, description, placeholder, successMessage }: {
  messagePrefix?: string
  title?: string
  description?: string
  placeholder?: string
  successMessage?: string
}) {
  const t = useTranslations('contactPage')
  const tCommon = useTranslations('common')
  const tApiErrors = useTranslations('errors.api')
  const { user, isAuthenticated } = useAuth()
  const [email, setEmail] = useState('')
  const [message, setMessage] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState('')

  const submitting = useRef(false)
  const maxMessageLength = MESSAGE_CONSTRAINTS.MAX_LENGTH - messagePrefix.length

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
    if (!isValidEmail(email)) {
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
    if (message.length > maxMessageLength) {
      return t('validation.messageTooLong', { max: maxMessageLength })
    }
    return null
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (submitting.current) return
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

    submitting.current = true
    setIsLoading(true)

    try {
      const response = await api.submitContactForm(email.trim(), messagePrefix + message.trim())
      setSuccess(successMessage ?? response.message)
      setMessage('') // Clear message on success, keep email
    } catch (err) {
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tApiErrors))
      } else {
        setError(err instanceof Error ? getTranslatedApiError(err, tApiErrors) : t('errors.sendFailed'))
      }
    } finally {
      submitting.current = false
      setIsLoading(false)
    }
  }
  return (
    <Card className="w-full max-w-xl">
      <CardHeader className="space-y-1">
        <CardTitle className="text-xl">
          {title ?? t('form.title')}
        </CardTitle>
        <CardDescription id="contact-description">
          {description ?? t('form.description')}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {error && (
          <ErrorDisplay message={error} variant="inline" className="mb-4" />
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
              aria-describedby="contact-description contact-count"
              placeholder={placeholder ?? t('form.messagePlaceholder')}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              required
              disabled={isLoading}
              className="min-h-[120px]"
            />
            <p id="contact-count" className="text-xs text-muted-foreground">
              {t('form.characterCount', { count: message.length, max: maxMessageLength })}
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
  )
}
