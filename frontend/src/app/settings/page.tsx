"use client"

import { useState, useEffect } from "react"
import { useRouter } from "next/navigation"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ArrowLeft, CreditCard, Globe } from "lucide-react"
import Link from "next/link"
import { useAuth } from "@/contexts/auth-context"
import { api } from "@/lib/api"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { SUPPORTED_CURRENCIES } from "@/lib/currencies"

export default function SettingsPage() {
  const router = useRouter()
  const { isAuthenticated, isLoading: authLoading, isCloudMode, billingStatus, user } = useAuth()
  const [selectedCurrency, setSelectedCurrency] = useState<string>('USD')
  const [isUpdating, setIsUpdating] = useState(false)
  const [userPreferences, setUserPreferences] = useState<{ preferred_fiat_currency: string } | null>(null)

  // Redirect unauthenticated users to sign-in when in SAAS mode
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
      await api.updateUserPreferences(currency)
      setUserPreferences({ preferred_fiat_currency: currency })
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

  // Return null while redirecting unauthenticated users in SAAS mode
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
