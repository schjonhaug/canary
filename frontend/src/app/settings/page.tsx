"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Globe, Bell, Languages } from "lucide-react"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { SUPPORTED_CURRENCIES } from "@/lib/currencies"
import { locales, localeNames, type Locale } from "@/i18n/config"
import { getStoredLocale, setStoredLocale } from "@/lib/locale"
import { useTranslations } from "next-intl"

export default function SettingsPage() {
  const router = useRouter()
  const t = useTranslations('settings')
  const tCommon = useTranslations('common')
  const { isAuthenticated, isLoading: authLoading, isCloudMode, user } = useAuth()
  const [selectedCurrency, setSelectedCurrency] = useState<string>('USD')
  const [currentLocale, setCurrentLocale] = useState<Locale>('en-US')
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

  // Initialize locale from cookie
  useEffect(() => {
    setCurrentLocale(getStoredLocale())
  }, [])

  const handleLanguageChange = async (locale: Locale) => {
    setStoredLocale(locale)
    setCurrentLocale(locale)

    // Sync to backend for authenticated users
    if (isAuthenticated) {
      try {
        await api.updateUserPreferences({ preferred_language: locale })
      } catch (error) {
        console.error('Failed to sync language preference:', error)
      }
    }

    // Refresh to apply new locale
    router.refresh()
  }

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
          <p className="mt-4 text-gray-600">{tCommon('loading')}</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in cloud mode
  if (isCloudMode && !isAuthenticated) {
    return null
  }

  return (
    <div className="space-y-6">
      {/* Page Title */}
      <h2 className="text-2xl font-semibold">{t('title')}</h2>

      <div className="max-w-4xl space-y-6">
        {/* Display Preferences */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Globe className="h-5 w-5" />
              {t('display.title')}
            </CardTitle>
            <CardDescription>
              {t('display.description')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <Label htmlFor="currency">{t('display.currencyLabel')}</Label>
                <Select
                  value={selectedCurrency}
                  onValueChange={handleCurrencyChange}
                  disabled={isUpdating || (isCloudMode && user?.is_demo)}
                >
                  <SelectTrigger id="currency" className="w-full">
                    <SelectValue placeholder={t('display.currencyPlaceholder')} />
                  </SelectTrigger>
                  <SelectContent>
                    {SUPPORTED_CURRENCIES.map((currency) => (
                      <SelectItem key={currency.code} value={currency.code}>
                        <span className="flex items-center gap-2">
                          <span className="font-mono text-sm">{currency.code}</span>
                          <span>{t(`currencies.${currency.code}`)}</span>
                          <span className="text-muted-foreground">({currency.symbol})</span>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-sm text-muted-foreground mt-2">
                  {t('display.currencyNote')}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Language Settings */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Languages className="h-5 w-5" />
              {t('language.title')}
            </CardTitle>
            <CardDescription>
              {t('language.description')}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <Label htmlFor="language">{t('language.label')}</Label>
                <Select
                  value={currentLocale}
                  onValueChange={(value) => handleLanguageChange(value as Locale)}
                  disabled={isCloudMode && user?.is_demo}
                >
                  <SelectTrigger id="language" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {[...locales].sort((a, b) => localeNames[a].localeCompare(localeNames[b])).map((locale) => (
                      <SelectItem key={locale} value={locale}>
                        {localeNames[locale]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
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
                {t('ntfy.title')}
              </CardTitle>
              <CardDescription>
                {t('ntfy.description')}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-6">
                {/* Server URL */}
                <div>
                  <Label htmlFor="ntfy-server">{t('ntfy.serverLabel')}</Label>
                  <div className="flex gap-2 mt-1">
                    <Input
                      id="ntfy-server"
                      type="url"
                      placeholder={t('ntfy.serverPlaceholder')}
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
                      {isUpdatingNtfy ? tCommon('saving') : tCommon('save')}
                    </Button>
                  </div>
                  {ntfyError && (
                    <p className="text-sm text-red-500 mt-1">{ntfyError}</p>
                  )}
                  {ntfySuccess && (
                    <p className="text-sm text-green-500 mt-1">{tCommon('savedSuccessfully')}</p>
                  )}
                  <p className="text-sm text-muted-foreground mt-2">
                    {t('ntfy.serverNoteBefore')}
                    <a
                      href="https://ntfy.sh/docs/install/"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-primary hover:underline"
                    >
                      {t('ntfy.selfHostLink')}
                    </a>
                    {t('ntfy.serverNoteAfter')}
                  </p>
                </div>

                {/* Authentication - only show if custom server URL is set */}
                {ntfyServerUrl && ntfyServerUrl !== 'https://ntfy.sh' && (
                  <div className="border-t pt-4">
                    <Label>{t('ntfy.auth.title')}</Label>
                    <p className="text-sm text-muted-foreground mb-3">
                      {userPreferences?.ntfy_has_access_token
                        ? t('ntfy.auth.configured.token')
                        : userPreferences?.ntfy_has_credentials
                          ? t('ntfy.auth.configured.credentials', { username: userPreferences.ntfy_username ?? '' })
                          : t('ntfy.auth.configured.none')}
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
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="none">{t('ntfy.auth.type.none')}</SelectItem>
                          <SelectItem value="token">{t('ntfy.auth.type.token')}</SelectItem>
                          <SelectItem value="basic">{t('ntfy.auth.type.basic')}</SelectItem>
                        </SelectContent>
                      </Select>

                      {ntfyAuthType === 'token' && (
                        <div>
                          <Label htmlFor="ntfy-token">{t('ntfy.auth.tokenLabel')}</Label>
                          <Input
                            id="ntfy-token"
                            type="password"
                            placeholder={userPreferences?.ntfy_has_access_token ? '••••••••' : t('ntfy.auth.tokenPlaceholder')}
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
                            <Label htmlFor="ntfy-username">{t('ntfy.auth.usernameLabel')}</Label>
                            <Input
                              id="ntfy-username"
                              type="text"
                              placeholder={t('ntfy.auth.usernamePlaceholder')}
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
                            <Label htmlFor="ntfy-password">{t('ntfy.auth.passwordLabel')}</Label>
                            <Input
                              id="ntfy-password"
                              type="password"
                              placeholder={userPreferences?.ntfy_has_credentials ? '••••••••' : t('ntfy.auth.passwordPlaceholder')}
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
                        {isUpdatingNtfyAuth ? tCommon('saving') : t('ntfy.auth.saveAuth')}
                      </Button>

                      {ntfyAuthError && (
                        <p className="text-sm text-red-500">{ntfyAuthError}</p>
                      )}
                      {ntfyAuthSuccess && (
                        <p className="text-sm text-green-500">{t('ntfy.auth.authSaved')}</p>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        )}

      </div>
    </div>
  )
}
