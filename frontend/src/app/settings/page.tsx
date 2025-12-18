"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ArrowLeft, CreditCard, Globe, Bell } from "lucide-react"
import Link from "next/link"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { SUPPORTED_CURRENCIES } from "@/lib/currencies"

export default function SettingsPage() {
  const router = useRouter()
  const { isAuthenticated, isLoading: authLoading, isCloudMode, billingStatus, user } = useAuth()
  const [selectedCurrency, setSelectedCurrency] = useState<string>('USD')
  const [ntfyServerUrl, setNtfyServerUrl] = useState<string>('')
  const [savedNtfyUrl, setSavedNtfyUrl] = useState<string>('')
  const [isUpdating, setIsUpdating] = useState(false)
  const [isUpdatingNtfy, setIsUpdatingNtfy] = useState(false)
  const [ntfyError, setNtfyError] = useState<string | null>(null)
  const [ntfySuccess, setNtfySuccess] = useState(false)
  const [userPreferences, setUserPreferences] = useState<{
    preferred_fiat_currency: string;
    ntfy_server_url: string | null;
    ntfy_has_access_token: boolean;
    ntfy_has_credentials: boolean;
    ntfy_username: string | null;
  } | null>(null)

  // ntfy authentication state
  const [ntfyAuthType, setNtfyAuthType] = useState<'none' | 'token' | 'basic'>('none')
  const [ntfyAccessToken, setNtfyAccessToken] = useState<string>('')
  const [ntfyUsername, setNtfyUsername] = useState<string>('')
  const [ntfyPassword, setNtfyPassword] = useState<string>('')
  const [isUpdatingNtfyAuth, setIsUpdatingNtfyAuth] = useState(false)
  const [ntfyAuthError, setNtfyAuthError] = useState<string | null>(null)
  const [ntfyAuthSuccess, setNtfyAuthSuccess] = useState(false)

  // Redirect unauthenticated users to sign-in when in cloud mode
  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) {
      router.push('/sign-in')
    }
  }, [isCloudMode, isAuthenticated, authLoading, router])

  // Fetch user preferences on mount
  useEffect(() => {
    const fetchPreferences = async () => {
      try {
        const prefs = await api.getUserPreferences()
        setUserPreferences(prefs)
        setSelectedCurrency(prefs.preferred_fiat_currency)
        setNtfyServerUrl(prefs.ntfy_server_url || '')
        setSavedNtfyUrl(prefs.ntfy_server_url || '')

        // Set auth type based on what's configured
        if (prefs.ntfy_has_access_token) {
          setNtfyAuthType('token')
        } else if (prefs.ntfy_has_credentials) {
          setNtfyAuthType('basic')
          setNtfyUsername(prefs.ntfy_username || '')
        } else {
          setNtfyAuthType('none')
        }
      } catch (error) {
        console.error('Failed to fetch user preferences:', error)
        // Default to USD if fetching fails
        setSelectedCurrency('USD')
      }
    }

    if (isAuthenticated) {
      fetchPreferences()
    }
  }, [isAuthenticated])

  const handleCurrencyChange = async (currency: string) => {
    setSelectedCurrency(currency)
    setIsUpdating(true)

    try {
      const result = await api.updateUserPreferences({ preferred_fiat_currency: currency })
      setUserPreferences(result)
    } catch (error) {
      console.error('Failed to update currency preference:', error)
      // Revert on error
      if (userPreferences) {
        setSelectedCurrency(userPreferences.preferred_fiat_currency)
      }
    } finally {
      setIsUpdating(false)
    }
  }

  const handleNtfyServerSave = async () => {
    setIsUpdatingNtfy(true)
    setNtfyError(null)
    setNtfySuccess(false)

    try {
      const result = await api.updateUserPreferences({ ntfy_server_url: ntfyServerUrl || '' })
      setUserPreferences(result)
      setSavedNtfyUrl(result.ntfy_server_url || '')
      setNtfySuccess(true)
      // Clear success message after 3 seconds
      setTimeout(() => setNtfySuccess(false), 3000)
    } catch (error) {
      console.error('Failed to update ntfy server URL:', error)
      setNtfyError(error instanceof Error ? error.message : 'Failed to save')
      // Revert on error
      setNtfyServerUrl(savedNtfyUrl)
    } finally {
      setIsUpdatingNtfy(false)
    }
  }

  const hasNtfyChanges = ntfyServerUrl !== savedNtfyUrl

  const handleNtfyAuthSave = async () => {
    setIsUpdatingNtfyAuth(true)
    setNtfyAuthError(null)
    setNtfyAuthSuccess(false)

    try {
      let updateData: {
        ntfy_access_token?: string;
        ntfy_username?: string;
        ntfy_password?: string;
      } = {}

      if (ntfyAuthType === 'none') {
        // Clear all auth
        updateData = {
          ntfy_access_token: '',
          ntfy_username: '',
          ntfy_password: '',
        }
      } else if (ntfyAuthType === 'token') {
        if (!ntfyAccessToken.trim()) {
          setNtfyAuthError('Access token is required')
          setIsUpdatingNtfyAuth(false)
          return
        }
        updateData = { ntfy_access_token: ntfyAccessToken.trim() }
      } else if (ntfyAuthType === 'basic') {
        if (!ntfyUsername.trim() || !ntfyPassword.trim()) {
          setNtfyAuthError('Both username and password are required')
          setIsUpdatingNtfyAuth(false)
          return
        }
        updateData = {
          ntfy_username: ntfyUsername.trim(),
          ntfy_password: ntfyPassword.trim(),
        }
      }

      const result = await api.updateUserPreferences(updateData)
      setUserPreferences(result)

      // Clear sensitive fields after save
      setNtfyAccessToken('')
      setNtfyPassword('')

      setNtfyAuthSuccess(true)
      setTimeout(() => setNtfyAuthSuccess(false), 3000)
    } catch (error) {
      console.error('Failed to update ntfy authentication:', error)
      setNtfyAuthError(error instanceof Error ? error.message : 'Failed to save')
    } finally {
      setIsUpdatingNtfyAuth(false)
    }
  }

  // Show loading state while auth is loading
  if (authLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">Loading...</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in cloud mode
  if (isCloudMode && !isAuthenticated) {
    return null
  }

  return (
    <div className="container mx-auto px-4 py-8 max-w-4xl">
      <div className="mb-6">
        <Link href="/wallets">
          <Button variant="ghost" size="sm" className="gap-2">
            <ArrowLeft size={16} />
            Back to Wallets
          </Button>
        </Link>
      </div>

      <h1 className="text-3xl font-bold mb-8">Settings</h1>

      <div className="space-y-6">
        {/* Display Preferences */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Globe className="h-5 w-5" />
              Display Preferences
            </CardTitle>
            <CardDescription>
              Customize how Bitcoin values are displayed in your wallets
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <Label htmlFor="currency">Fiat Currency</Label>
                <Select
                  value={selectedCurrency}
                  onValueChange={handleCurrencyChange}
                  disabled={isUpdating || (isCloudMode && user?.is_demo)}
                >
                  <SelectTrigger id="currency" className="w-full">
                    <SelectValue placeholder="Select a currency" />
                  </SelectTrigger>
                  <SelectContent>
                    {SUPPORTED_CURRENCIES.map((currency) => (
                      <SelectItem key={currency.code} value={currency.code}>
                        <span className="flex items-center gap-2">
                          <span className="font-mono text-sm">{currency.code}</span>
                          <span>{currency.name}</span>
                          <span className="text-muted-foreground">({currency.symbol})</span>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-sm text-muted-foreground mt-2">
                  Exchange rates are updated every 10 minutes from CoinGecko
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* ntfy Server Settings - Only show in self-hosted mode */}
        {!isCloudMode && (
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Bell className="h-5 w-5" />
                Push Notifications
              </CardTitle>
              <CardDescription>
                Configure your ntfy server for push notifications
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-6">
                {/* Server URL */}
                <div>
                  <Label htmlFor="ntfy-server">ntfy Server URL</Label>
                  <div className="flex gap-2 mt-1">
                    <Input
                      id="ntfy-server"
                      type="url"
                      placeholder="https://ntfy.sh"
                      value={ntfyServerUrl}
                      onChange={(e) => {
                        setNtfyServerUrl(e.target.value)
                        setNtfyError(null)
                        setNtfySuccess(false)
                      }}
                      disabled={isUpdatingNtfy}
                      className="flex-1"
                    />
                    <Button
                      onClick={handleNtfyServerSave}
                      disabled={isUpdatingNtfy || !hasNtfyChanges}
                    >
                      {isUpdatingNtfy ? 'Saving...' : 'Save'}
                    </Button>
                  </div>
                  {ntfyError && (
                    <p className="text-sm text-red-500 mt-1">{ntfyError}</p>
                  )}
                  {ntfySuccess && (
                    <p className="text-sm text-green-500 mt-1">Saved successfully!</p>
                  )}
                  <p className="text-sm text-muted-foreground mt-2">
                    Leave empty to use the public ntfy.sh server. You can{' '}
                    <a
                      href="https://ntfy.sh/docs/install/"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-primary hover:underline"
                    >
                      self-host ntfy
                    </a>{' '}
                    for complete privacy.
                  </p>
                </div>

                {/* Authentication - only show if custom server URL is set */}
                {ntfyServerUrl && ntfyServerUrl !== 'https://ntfy.sh' && (
                  <div className="border-t pt-4">
                    <Label>Authentication</Label>
                    <p className="text-sm text-muted-foreground mb-3">
                      {userPreferences?.ntfy_has_access_token
                        ? 'Access token configured'
                        : userPreferences?.ntfy_has_credentials
                          ? `Username/password configured (${userPreferences.ntfy_username})`
                          : 'No authentication configured'}
                    </p>

                    <div className="space-y-3">
                      <Select
                        value={ntfyAuthType}
                        onValueChange={(value: 'none' | 'token' | 'basic') => {
                          setNtfyAuthType(value)
                          setNtfyAuthError(null)
                          setNtfyAuthSuccess(false)
                        }}
                        disabled={isUpdatingNtfyAuth}
                      >
                        <SelectTrigger>
                          <SelectValue placeholder="Select authentication method" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="none">No authentication</SelectItem>
                          <SelectItem value="token">Access Token</SelectItem>
                          <SelectItem value="basic">Username &amp; Password</SelectItem>
                        </SelectContent>
                      </Select>

                      {ntfyAuthType === 'token' && (
                        <div>
                          <Label htmlFor="ntfy-token">Access Token</Label>
                          <Input
                            id="ntfy-token"
                            type="password"
                            placeholder={userPreferences?.ntfy_has_access_token ? '••••••••' : 'tk_...'}
                            value={ntfyAccessToken}
                            onChange={(e) => {
                              setNtfyAccessToken(e.target.value)
                              setNtfyAuthError(null)
                            }}
                            disabled={isUpdatingNtfyAuth}
                            className="mt-1"
                          />
                        </div>
                      )}

                      {ntfyAuthType === 'basic' && (
                        <>
                          <div>
                            <Label htmlFor="ntfy-username">Username</Label>
                            <Input
                              id="ntfy-username"
                              type="text"
                              placeholder="Username"
                              value={ntfyUsername}
                              onChange={(e) => {
                                setNtfyUsername(e.target.value)
                                setNtfyAuthError(null)
                              }}
                              disabled={isUpdatingNtfyAuth}
                              className="mt-1"
                            />
                          </div>
                          <div>
                            <Label htmlFor="ntfy-password">Password</Label>
                            <Input
                              id="ntfy-password"
                              type="password"
                              placeholder={userPreferences?.ntfy_has_credentials ? '••••••••' : 'Password'}
                              value={ntfyPassword}
                              onChange={(e) => {
                                setNtfyPassword(e.target.value)
                                setNtfyAuthError(null)
                              }}
                              disabled={isUpdatingNtfyAuth}
                              className="mt-1"
                            />
                          </div>
                        </>
                      )}

                      <Button
                        onClick={handleNtfyAuthSave}
                        disabled={isUpdatingNtfyAuth}
                        className="w-full"
                      >
                        {isUpdatingNtfyAuth ? 'Saving...' : 'Save Authentication'}
                      </Button>

                      {ntfyAuthError && (
                        <p className="text-sm text-red-500">{ntfyAuthError}</p>
                      )}
                      {ntfyAuthSuccess && (
                        <p className="text-sm text-green-500">Authentication saved!</p>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        )}

        {/* Subscription Management */}
        {isCloudMode && (
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <CreditCard className="h-5 w-5" />
                Subscription
              </CardTitle>
              <CardDescription>
                Manage your subscription and billing settings
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="font-medium">Current Plan</p>
                    <p className="text-sm text-muted-foreground capitalize">
                      {billingStatus?.subscription_tier || 'Personal'} Plan
                    </p>
                  </div>
                  <Link href="/settings/subscription">
                    <Button variant="outline">Manage Subscription</Button>
                  </Link>
                </div>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  )
}
