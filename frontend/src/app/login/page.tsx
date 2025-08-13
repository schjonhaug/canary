'use client'

import { useState } from 'react'
import { useAuth } from '@/contexts/auth-context'
import { api } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Phone, ArrowRight, Loader2, User, Shield } from 'lucide-react'
import Image from 'next/image'

export default function LoginPage() {
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [name, setName] = useState('')
  const [mode, setMode] = useState<'login' | 'register' | 'forgot-password'>('login')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const { register, login } = useAuth()

  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setMessage('')
    setIsLoading(true)

    try {
      await register(email, password, name)
      setMessage('Registration successful! Please check your email to verify your account.')
      setMode('login')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Registration failed')
    } finally {
      setIsLoading(false)
    }
  }

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setMessage('')
    setIsLoading(true)

    try {
      await login(email, password)
      // Navigation is handled by the login function in auth context
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed')
    } finally {
      setIsLoading(false)
    }
  }

  const handleForgotPassword = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setMessage('')
    setIsLoading(true)

    try {
      await api.forgotPassword(email)
      setMessage('If an account with that email exists, a password reset link has been sent.')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to send reset email')
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
      setError(err instanceof Error ? err.message : 'Failed to login')
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
              alt="Canary Logo"
              width={48}
              height={48}
              className="h-12 w-12"
            />
          </div>
          <CardTitle className="text-2xl font-bold text-center">
            Welcome to Canary
          </CardTitle>
          <CardDescription className="text-center">
            {mode === 'login' 
              ? 'Sign in to your account'
              : mode === 'register'
              ? 'Create a new account'
              : 'Enter your email to reset your password'
            }
          </CardDescription>
        </CardHeader>
        <CardContent>
          {error && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          
          {message && (
            <Alert className="mb-4">
              <AlertDescription>{message}</AlertDescription>
            </Alert>
          )}

          {/* Development Mode Quick Login */}
          {process.env.NODE_ENV === 'development' && mode === 'login' && (
            <div className="mb-6 p-4 bg-blue-50 border border-blue-200 rounded-lg">
              <div className="flex items-center gap-2 mb-3">
                <Shield className="h-4 w-4 text-blue-600" />
                <span className="text-sm font-medium text-blue-800">Development Mode</span>
              </div>
              <div className="space-y-2">
                <div className="grid grid-cols-1 gap-2">
                  {[
                    { email: 'delivered+admin@resend.dev', label: 'Admin (Pro + Admin)' },
                    { email: 'delivered+alice@resend.dev', label: 'Alice (Personal)' },
                    { email: 'delivered+bob@resend.dev', label: 'Bob (Pro)' }
                  ].map(({ email, label }) => (
                    <Button
                      key={email}
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => handleDevLogin(email)}
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

          {mode === 'login' ? (
            <form onSubmit={handleLogin} className="space-y-4">
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
                <Label htmlFor="password">Password</Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter your password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  disabled={isLoading}
                />
              </div>
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || !email || !password}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Signing in...
                  </>
                ) : (
                  'Sign in'
                )}
              </Button>
              <div className="flex flex-col gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  className="w-full"
                  onClick={() => {
                    setMode('register')
                    setError('')
                    setMessage('')
                  }}
                  disabled={isLoading}
                >
                  Don't have an account? Sign up
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  className="w-full"
                  onClick={() => {
                    setMode('forgot-password')
                    setError('')
                    setMessage('')
                  }}
                  disabled={isLoading}
                >
                  Forgot your password?
                </Button>
              </div>
            </form>
          ) : mode === 'register' ? (
            <form onSubmit={handleRegister} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="name">Name</Label>
                <Input
                  id="name"
                  type="text"
                  placeholder="Enter your name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                  disabled={isLoading}
                />
              </div>
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
                <Label htmlFor="password">Password</Label>
                <Input
                  id="password"
                  type="password"
                  placeholder="Enter a strong password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  disabled={isLoading}
                  minLength={6}
                />
                <p className="text-sm text-gray-500">Password must be at least 6 characters long</p>
              </div>
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || !email || !password || !name}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Creating account...
                  </>
                ) : (
                  'Create account'
                )}
              </Button>
              <Button
                type="button"
                variant="ghost"
                className="w-full"
                onClick={() => {
                  setMode('login')
                  setError('')
                  setMessage('')
                }}
                disabled={isLoading}
              >
                Already have an account? Sign in
              </Button>
            </form>
          ) : (
            <form onSubmit={handleForgotPassword} className="space-y-4">
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
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || !email}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Sending...
                  </>
                ) : (
                  'Send reset link'
                )}
              </Button>
              <Button
                type="button"
                variant="ghost"
                className="w-full"
                onClick={() => {
                  setMode('login')
                  setError('')
                  setMessage('')
                }}
                disabled={isLoading}
              >
                Back to sign in
              </Button>
            </form>
          )}
        </CardContent>
      </Card>
    </div>
  )
}