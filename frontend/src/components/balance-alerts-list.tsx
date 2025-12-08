"use client"

import { useState, useEffect } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Bell,
  Plus,
  Trash2,
  TrendingUp,
  TrendingDown,
  Target
} from "lucide-react"
import { api } from "@/lib/api"
import { BalanceAlert, CreateBalanceAlertRequest } from "@/types"
import {
  satsToBtc,
  btcToSats,
  formatBtcAmount,
  parseBtcInput,
  getBtcPlaceholder
} from "@/lib/utils"
import { formatFiatAmount } from "@/lib/currencies"
import { useAuth } from "@/contexts/auth-context"

interface BalanceAlertsListProps {
  walletChecksum: string
  balanceAlerts: BalanceAlert[]
}

const ALERT_TYPE_OPTIONS = [
  {
    value: 'above',
    label: 'Above',
    icon: TrendingUp,
    description: 'Alert when balance goes above this amount'
  },
  {
    value: 'below',
    label: 'Below',
    icon: TrendingDown,
    description: 'Alert when balance goes below this amount'
  },
  {
    value: 'equals',
    label: 'Equals',
    icon: Target,
    description: 'Alert when balance equals this amount (e.g., wallet drain)'
  },
] as const

export function BalanceAlertsList({
  walletChecksum,
  balanceAlerts
}: BalanceAlertsListProps) {
  const { user, isCloudMode } = useAuth()

  // Use local state for optimistic updates
  const [localAlerts, setLocalAlerts] = useState<BalanceAlert[]>(balanceAlerts)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  // Form state for creating new alerts
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [alertType, setAlertType] = useState<'above' | 'below' | 'equals'>('below')
  const [thresholdInput, setThresholdInput] = useState('')
  const [currencyType, setCurrencyType] = useState<'btc' | 'fiat'>('btc')
  const [preferredCurrency, setPreferredCurrency] = useState<string>('USD')

  // Sync local alerts with prop when it changes (from polling)
  useEffect(() => {
    setLocalAlerts(balanceAlerts)
  }, [balanceAlerts])

  // Load user's preferred currency
  useEffect(() => {
    const fetchPreferredCurrency = async () => {
      try {
        const prefs = await api.getUserPreferences()
        setPreferredCurrency(prefs.preferred_fiat_currency)
      } catch (err) {
        console.error('Failed to fetch user preferences:', err)
        // Keep default USD if fetch fails
      }
    }
    fetchPreferredCurrency()
  }, [])

  const handleCreateAlert = async () => {
    if (currencyType === 'btc') {
      // BTC threshold validation
      const thresholdBtc = parseBtcInput(thresholdInput)
      if (thresholdBtc === null) {
        setError('Please enter a valid Bitcoin amount')
        return
      }

      // Check for negative amounts
      if (thresholdBtc < 0) {
        setError('Amount cannot be negative')
        return
      }

      const thresholdSats = btcToSats(thresholdBtc)

      // Check for "below 0" alerts (logically impossible)
      if (alertType === 'below' && thresholdSats === 0) {
        setError('Cannot create alert for "below 0" - balance cannot go below zero')
        return
      }

      // Check for duplicate alert
      const duplicate = localAlerts.find(alert =>
        alert.alert_type === alertType &&
        alert.threshold_sats === thresholdSats &&
        !alert.threshold_currency
      )

      if (duplicate) {
        setError('An alert with this type and amount already exists')
        return
      }

      setIsSubmitting(true)
      setError(null)

      try {
        const alertData: CreateBalanceAlertRequest = {
          threshold_sats: thresholdSats,
          alert_type: alertType
        }

        const newAlert = await api.createBalanceAlert(walletChecksum, alertData)
        setLocalAlerts(prev => [...prev, newAlert])
        setShowCreateForm(false)
        setThresholdInput('')
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to create balance alert')
      } finally {
        setIsSubmitting(false)
      }
    } else {
      // Fiat threshold validation
      const thresholdFiat = parseFloat(thresholdInput)
      if (isNaN(thresholdFiat) || thresholdFiat <= 0) {
        setError('Please enter a valid positive amount')
        return
      }

      // Check for "below 0" alerts (logically impossible)
      if (alertType === 'below' && thresholdFiat === 0) {
        setError('Cannot create alert for "below 0" - balance cannot go below zero')
        return
      }

      // Check for duplicate alert
      const duplicate = localAlerts.find(alert =>
        alert.alert_type === alertType &&
        alert.threshold_currency === preferredCurrency &&
        alert.threshold_fiat_amount === thresholdFiat
      )

      if (duplicate) {
        setError('An alert with this type and amount already exists')
        return
      }

      setIsSubmitting(true)
      setError(null)

      try {
        const alertData: CreateBalanceAlertRequest = {
          alert_type: alertType,
          threshold_currency: preferredCurrency,
          threshold_fiat_amount: thresholdFiat
        }

        const newAlert = await api.createBalanceAlert(walletChecksum, alertData)
        setLocalAlerts(prev => [...prev, newAlert])
        setShowCreateForm(false)
        setThresholdInput('')
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to create balance alert')
      } finally {
        setIsSubmitting(false)
      }
    }
  }

  const handleDeleteAlert = async (alertId: string) => {
    try {
      await api.deleteBalanceAlert(alertId)
      setLocalAlerts(prev => prev.filter(alert => alert.id !== alertId))
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete alert')
    }
  }

  const getAlertTypeIcon = (type: string) => {
    return ALERT_TYPE_OPTIONS.find(opt => opt.value === type)?.icon || Target
  }

  const formatAlertDescription = (alert: BalanceAlert) => {
    const typeLabel = ALERT_TYPE_OPTIONS.find(opt => opt.value === alert.alert_type)?.label || alert.alert_type

    if (alert.threshold_currency && alert.threshold_fiat_amount) {
      // Fiat alert - formatFiatAmount already includes currency symbol
      const formattedAmount = formatFiatAmount(alert.threshold_fiat_amount, alert.threshold_currency)
      return `${typeLabel} ${formattedAmount}`
    } else {
      // BTC alert
      const btcAmount = formatBtcAmount(satsToBtc(alert.threshold_sats))
      return `${typeLabel} ${btcAmount} BTC`
    }
  }


  return (
    <div className="space-y-3">


      {/* Existing Alerts */}
      <div>
        <div className="flex items-center justify-between mb-2">
          <h4 className="text-sm font-medium text-muted-foreground">Balance Alerts</h4>
          {!showCreateForm && !(isCloudMode && (user?.is_admin || user?.is_demo)) && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowCreateForm(true)}
              className="h-6 px-2 text-xs gap-1"
            >
              <Plus className="h-3 w-3" />
              New
            </Button>
          )}
        </div>

        {localAlerts.length === 0 ? (
          <div className="text-center text-muted-foreground text-xs py-3">
            <Bell className="h-4 w-4 mx-auto mb-1 opacity-50" />
            <p>No alerts set</p>
          </div>
        ) : (
          <div className="space-y-2">
            {localAlerts.map((alert) => {
              const AlertIcon = getAlertTypeIcon(alert.alert_type)

              return (
                <div key={alert.id} className="p-2 border rounded text-xs">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      <AlertIcon className="h-3 w-3 text-muted-foreground flex-shrink-0" />
                      <div className="min-w-0">
                        <div className="font-medium truncate">
                          {formatAlertDescription(alert)}
                        </div>
                        {alert.last_triggered_at && (
                          <div className="text-xs text-muted-foreground">
                            Last fired {new Date(alert.last_triggered_at * 1000).toLocaleDateString()}
                          </div>
                        )}
                      </div>
                    </div>

                    {!(isCloudMode && (user?.is_admin || user?.is_demo)) && (
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteAlert(alert.id)}
                          className="h-5 w-5 p-0 text-muted-foreground hover:text-red-600"
                          title="Delete alert"
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    )}
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>

      {/* Create Alert Form */}
      {showCreateForm && (
        <div className="p-3 border rounded space-y-3">
          <div className="flex items-center justify-between">
            <h4 className="text-xs font-medium">Create New Alert</h4>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowCreateForm(false)}
              className="h-5 w-5 p-0"
            >
              ×
            </Button>
          </div>

          <div className="space-y-3">
            <div>
              <Label className="text-xs">Alert Type</Label>
              <RadioGroup
                value={alertType}
                onValueChange={(value) => setAlertType(value as typeof alertType)}
                className="mt-2"
              >
                {ALERT_TYPE_OPTIONS.map((option) => {
                  const IconComponent = option.icon
                  return (
                    <div key={option.value} className="flex items-center space-x-2">
                      <RadioGroupItem value={option.value} id={option.value} />
                      <Label
                        htmlFor={option.value}
                        className="text-xs flex items-center gap-2 cursor-pointer flex-1"
                      >
                        <IconComponent className="h-3 w-3" />
                        <span>{option.label}</span>
                      </Label>
                    </div>
                  )
                })}
              </RadioGroup>
              <p className="text-xs text-muted-foreground mt-1">
                {ALERT_TYPE_OPTIONS.find(opt => opt.value === alertType)?.description}
              </p>
            </div>

            <div>
              <Label className="text-xs">Currency Type</Label>
              <RadioGroup
                value={currencyType}
                onValueChange={(value) => {
                  setCurrencyType(value as 'btc' | 'fiat')
                  setThresholdInput('') // Clear input when switching types
                }}
                className="mt-2 flex gap-4"
              >
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="btc" id="currency-btc" />
                  <Label htmlFor="currency-btc" className="text-xs cursor-pointer">
                    Bitcoin (BTC)
                  </Label>
                </div>
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="fiat" id="currency-fiat" />
                  <Label htmlFor="currency-fiat" className="text-xs cursor-pointer">
                    Fiat Currency ({preferredCurrency})
                  </Label>
                </div>
              </RadioGroup>
            </div>

            <div>
              <Label htmlFor="threshold-amount" className="text-xs">
                {currencyType === 'btc' ? 'Bitcoin Amount' : `Amount (${preferredCurrency})`}
              </Label>
              <Input
                id="threshold-amount"
                value={thresholdInput}
                onChange={(e) => setThresholdInput(e.target.value)}
                placeholder={currencyType === 'btc' ? getBtcPlaceholder() : '1000'}
                disabled={isSubmitting}
                className="font-mono h-8 text-xs"
              />
              <p className="text-xs text-muted-foreground mt-1">
                {currencyType === 'btc' ? 'Enter amount in BTC' : `Enter amount in ${preferredCurrency}`}
              </p>
            </div>
          </div>

          {error && (
            <div className="text-xs text-red-600 bg-red-50 p-2 rounded border border-red-200">
              {error}
            </div>
          )}

          <div className="flex justify-end gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowCreateForm(false)}
              disabled={isSubmitting}
              className="h-7 px-2 text-xs"
            >
              Cancel
            </Button>
            <Button
              size="sm"
              onClick={handleCreateAlert}
              disabled={isSubmitting || !thresholdInput.trim()}
              className="h-7 px-2 text-xs"
            >
              {isSubmitting ? "Creating..." : "Create"}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}