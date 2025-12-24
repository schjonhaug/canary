'use client'

import { useState, useEffect } from 'react'
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
      return 'Email is required'
    }
    if (!EMAIL_REGEX.test(email)) {
      return 'Please enter a valid email address'
    }
    if (email.length > EMAIL_CONSTRAINTS.MAX_LENGTH) {
      return `Email must be less than ${EMAIL_CONSTRAINTS.MAX_LENGTH} characters`
    }
    return null
  }

  const validateMessage = (message: string): string | null => {
    if (!message.trim()) {
      return 'Message is required'
    }
    if (message.trim().length < MESSAGE_CONSTRAINTS.MIN_LENGTH) {
      return `Message must be at least ${MESSAGE_CONSTRAINTS.MIN_LENGTH} characters`
    }
    if (message.length > MESSAGE_CONSTRAINTS.MAX_LENGTH) {
      return `Message must be less than ${MESSAGE_CONSTRAINTS.MAX_LENGTH} characters`
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
        setError(err instanceof Error ? err.message : 'Failed to send message')
      }
    } finally {
      setIsLoading(false)
    }
  }

  // In self-hosted mode, show a simple message (no contact form needed)
  if (isSelfHostedMode) {
    return (
      <div className="space-y-6">
        <h2 className="text-2xl font-semibold">Contact</h2>
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>Self-Hosted Mode</CardTitle>
            <CardDescription>
              Contact form is not available in self-hosted mode.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Link href="/wallets">
              <Button variant="outline" className="w-full">
                Back to Wallets
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
      <h2 className="text-2xl font-semibold">Contact</h2>

      <Card className="max-w-md">
        <CardHeader className="space-y-1">
          <CardTitle className="text-xl">
            Get in Touch
          </CardTitle>
          <CardDescription>
            Have a question or feedback? We would love to hear from you.
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
              <Label htmlFor="email">Email</Label>
              <Input
                id="email"
                type="email"
                placeholder="your@email.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                disabled={isLoading}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="message">Message</Label>
              <Textarea
                id="message"
                placeholder="How can we help you?"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                required
                disabled={isLoading}
                className="min-h-[120px]"
              />
              <p className="text-xs text-muted-foreground">
                {message.length}/{MESSAGE_CONSTRAINTS.MAX_LENGTH} characters
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
                  Sending...
                </>
              ) : (
                'Send Message'
              )}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
