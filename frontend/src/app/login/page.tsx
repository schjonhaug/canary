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
  const [phone, setPhone] = useState('')
  const [otp, setOtp] = useState('')
  const [name, setName] = useState('')
  const [step, setStep] = useState<'phone' | 'otp' | 'name'>('phone')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const { sendOtp, setAuth } = useAuth()

  const handleSendOtp = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      await sendOtp(phone)
      setStep('otp')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to send OTP')
    } finally {
      setIsLoading(false)
    }
  }

  const handleVerifyOtp = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      // Try to verify OTP without name first
      const response = await api.verifyOtp(phone, otp)
      
      // Check if name is required
      if ('requires_name' in response && response.requires_name) {
        setStep('name')
        setIsLoading(false)
        return
      }
      
      // Otherwise, we already have a successful login response
      // Use setAuth to avoid making another API call
      await setAuth(response.token, response.user)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid OTP')
    } finally {
      setIsLoading(false)
    }
  }

  const handleSubmitName = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      const response = await api.updateUserProfile(name)
      // Update auth context with new user data and redirect
      await setAuth(localStorage.getItem('auth_token')!, response.user)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update profile')
    } finally {
      setIsLoading(false)
    }
  }

  const handleDevLogin = async (phoneNumber: string) => {
    setError('')
    setIsLoading(true)

    try {
      await sendOtp(phoneNumber)
      setPhone(phoneNumber)
      setStep('otp')
      // For dev mode, auto-fill the OTP
      setOtp('123456')
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
            {step === 'phone' 
              ? 'Enter your phone number to get started'
              : step === 'otp'
              ? 'Enter the verification code sent to your phone'
              : 'Welcome! Please enter your name to complete registration'
            }
          </CardDescription>
        </CardHeader>
        <CardContent>
          {error && (
            <Alert variant="destructive" className="mb-4">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          {/* Development Mode Quick Login */}
          {process.env.NODE_ENV === 'development' && step === 'phone' && (
            <div className="mb-6 p-4 bg-blue-50 border border-blue-200 rounded-lg">
              <div className="flex items-center gap-2 mb-3">
                <Shield className="h-4 w-4 text-blue-600" />
                <span className="text-sm font-medium text-blue-800">Development Mode</span>
              </div>
              <div className="space-y-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-full justify-start"
                  onClick={() => handleDevLogin('+4799999900')}
                  disabled={isLoading}
                >
                  <Shield className="mr-2 h-4 w-4" />
                  Admin Account
                </Button>
                <div className="grid grid-cols-1 gap-2">
                  {[
                    { phone: '+4799999901', label: '+47 999 99 901 (Alice)' },
                    { phone: '+4699999902', label: '+46 999 99 902 (Bob)' },
                    { phone: '+3399999903', label: '+33 999 99 903 (New user)' }
                  ].map(({ phone, label }) => (
                    <Button
                      key={phone}
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => handleDevLogin(phone)}
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

          {step === 'phone' ? (
            <form onSubmit={handleSendOtp} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="phone">Phone Number</Label>
                <div className="relative">
                  <Phone className="absolute left-3 top-3 h-4 w-4 text-gray-400" />
                  <Input
                    id="phone"
                    type="tel"
                    placeholder="+1 (555) 000-0000"
                    value={phone}
                    onChange={(e) => setPhone(e.target.value)}
                    className="pl-10"
                    required
                    disabled={isLoading}
                  />
                </div>
              </div>
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || !phone}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Sending...
                  </>
                ) : (
                  <>
                    Send Verification Code
                    <ArrowRight className="ml-2 h-4 w-4" />
                  </>
                )}
              </Button>
            </form>
          ) : step === 'otp' ? (
            <form onSubmit={handleVerifyOtp} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="otp">Verification Code</Label>
                <Input
                  id="otp"
                  type="text"
                  placeholder="000000"
                  value={otp}
                  onChange={(e) => setOtp(e.target.value.replace(/\D/g, '').slice(0, 6))}
                  className="text-center text-2xl tracking-widest"
                  required
                  disabled={isLoading}
                  autoFocus
                  maxLength={6}
                />
              </div>
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || otp.length !== 6}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Verifying...
                  </>
                ) : (
                  'Verify & Login'
                )}
              </Button>
              <Button
                type="button"
                variant="ghost"
                className="w-full"
                onClick={() => {
                  setStep('phone')
                  setOtp('')
                  setError('')
                }}
                disabled={isLoading}
              >
                Use a different number
              </Button>
            </form>
          ) : (
            <form onSubmit={handleSubmitName} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="name">Your Name</Label>
                <Input
                  id="name"
                  type="text"
                  placeholder="Enter your name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                  disabled={isLoading}
                  autoFocus
                />
              </div>
              <Button 
                type="submit" 
                className="w-full"
                disabled={isLoading || !name.trim()}
              >
                {isLoading ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Creating Account...
                  </>
                ) : (
                  'Create Account'
                )}
              </Button>
              <Button
                type="button"
                variant="ghost"
                className="w-full"
                onClick={() => {
                  setStep('phone')
                  setOtp('')
                  setName('')
                  setError('')
                }}
                disabled={isLoading}
              >
                Start over
              </Button>
            </form>
          )}
        </CardContent>
      </Card>
    </div>
  )
}