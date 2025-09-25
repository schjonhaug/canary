"use client"

import { useState, useEffect, useCallback } from "react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Badge } from "@/components/ui/badge"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Card, CardContent } from "@/components/ui/card"
import {
  Bell,
  Plus,
  Trash2,
  RotateCcw,
  CheckCircle,
  AlertTriangle,
  TrendingUp,
  TrendingDown,
  Target
} from "lucide-react"
import { api } from "@/lib/api"
import { BalanceAlert, CreateBalanceAlertRequest } from "@/types"
import {
  formatBitcoinAmount,
  satsToBtc,
  btcToSats,
  formatBtcAmount,
  parseBtcInput
} from "@/lib/utils"
import { LoadingSpinner } from "@/components/ui/loading-spinner"

interface BalanceAlertsModalProps {
  isOpen: boolean
  onClose: () => void
  walletChecksum: string
  currentBalance: number // in satoshis
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

export function BalanceAlertsModal({
  isOpen,
  onClose,
  walletChecksum,
  currentBalance
}: BalanceAlertsModalProps) {
  const [alerts, setAlerts] = useState<BalanceAlert[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  // Form state for creating new alerts
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [alertType, setAlertType] = useState<'above' | 'below' | 'equals'>('below')
  const [thresholdInput, setThresholdInput] = useState('')

  const loadAlerts = useCallback(async () => {
    setIsLoading(true)
    try {
      const alertList = await api.getBalanceAlerts(walletChecksum)
      setAlerts(alertList)
      setError(null)
    } catch (err) {
      console.error('Failed to load balance alerts:', err)
      setError(err instanceof Error ? err.message : 'Failed to load balance alerts')
    } finally {
      setIsLoading(false)
    }
  }, [walletChecksum])

  // Load alerts when modal opens
  useEffect(() => {
    if (isOpen) {
      loadAlerts()
      setShowCreateForm(false)
      setError(null)
      setThresholdInput('')
      setAlertType('below')
    }
  }, [isOpen, walletChecksum, loadAlerts])

  const handleCreateAlert = async () => {
    const thresholdBtc = parseBtcInput(thresholdInput)
    if (thresholdBtc === null) {
      setError('Please enter a valid Bitcoin amount')
      return
    }

    const thresholdSats = btcToSats(thresholdBtc)

    setIsSubmitting(true)
    setError(null)

    try {
      const alertData: CreateBalanceAlertRequest = {
        threshold_sats: thresholdSats,
        alert_type: alertType
      }

      const newAlert = await api.createBalanceAlert(walletChecksum, alertData)
      setAlerts(prev => [...prev, newAlert])
      setShowCreateForm(false)
      setThresholdInput('')
    } catch (err) {
      console.error('Failed to create balance alert:', err)
      setError(err instanceof Error ? err.message : 'Failed to create balance alert')
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleReactivateAlert = async (alertId: string) => {
    try {
      const updatedAlert = await api.reactivateBalanceAlert(alertId)
      setAlerts(prev => prev.map(alert =>
        alert.id === alertId ? updatedAlert : alert
      ))
      setError(null)
    } catch (err) {
      console.error('Failed to reactivate alert:', err)
      setError(err instanceof Error ? err.message : 'Failed to reactivate alert')
    }
  }

  const handleDeleteAlert = async (alertId: string) => {
    try {
      await api.deleteBalanceAlert(alertId)
      setAlerts(prev => prev.filter(alert => alert.id !== alertId))
      setError(null)
    } catch (err) {
      console.error('Failed to delete alert:', err)
      setError(err instanceof Error ? err.message : 'Failed to delete alert')
    }
  }

  const getAlertStatus = (alert: BalanceAlert) => {
    if (!alert.is_active) {
      return {
        status: 'fired',
        variant: 'destructive' as const,
        icon: AlertTriangle,
        label: 'Fired'
      }
    }
    return {
      status: 'active',
      variant: 'secondary' as const,
      icon: CheckCircle,
      label: 'Active'
    }
  }

  const getAlertTypeIcon = (type: string) => {
    return ALERT_TYPE_OPTIONS.find(opt => opt.value === type)?.icon || Target
  }

  const formatAlertDescription = (alert: BalanceAlert) => {
    const btcAmount = formatBtcAmount(satsToBtc(alert.threshold_sats))
    const typeLabel = ALERT_TYPE_OPTIONS.find(opt => opt.value === alert.alert_type)?.label || alert.alert_type

    return `${typeLabel} ${btcAmount} BTC`
  }

  // Quick preset for wallet drain (balance = 0)
  const handleWalletDrainPreset = () => {
    setAlertType('equals')
    setThresholdInput('0')
    setShowCreateForm(true)
  }

  const handleClose = () => {
    setShowCreateForm(false)
    setError(null)
    setThresholdInput('')
    onClose()
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Bell className="h-5 w-5" />
            Balance Alerts
          </DialogTitle>
          <DialogDescription>
            Set up alerts to be notified when your wallet balance reaches specific thresholds.
            Alerts fire once and must be reactivated after triggering.
          </DialogDescription>
        </DialogHeader>

        {error && (
          <div className="text-sm text-red-600 bg-red-50 p-3 rounded-md border border-red-200">
            {error}
          </div>
        )}

        <div className="space-y-4">
          {/* Current Balance Display */}
          <Card>
            <CardContent className="pt-4">
              <div className="text-sm text-muted-foreground">Current Balance</div>
              <div className="text-xl font-bold font-mono">
                {formatBitcoinAmount(currentBalance)}
              </div>
            </CardContent>
          </Card>

          {/* Existing Alerts */}
          <div>
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-medium">Your Alerts</h3>
              {!showCreateForm && (
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleWalletDrainPreset}
                    className="gap-2"
                  >
                    <Target className="h-4 w-4" />
                    Wallet Drain Alert
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setShowCreateForm(true)}
                    className="gap-2"
                  >
                    <Plus className="h-4 w-4" />
                    Custom Alert
                  </Button>
                </div>
              )}
            </div>

            {isLoading ? (
              <div className="flex items-center justify-center py-8">
                <LoadingSpinner size="md" />
              </div>
            ) : alerts.length === 0 ? (
              <Card>
                <CardContent className="pt-6 pb-6 text-center text-muted-foreground">
                  <Bell className="h-8 w-8 mx-auto mb-2 opacity-50" />
                  <p>No balance alerts set up yet.</p>
                  <p className="text-xs mt-1">Create an alert to get notified when your balance changes.</p>
                </CardContent>
              </Card>
            ) : (
              <div className="space-y-2">
                {alerts.map((alert) => {
                  const status = getAlertStatus(alert)
                  const AlertIcon = getAlertTypeIcon(alert.alert_type)

                  return (
                    <Card key={alert.id} className="relative">
                      <CardContent className="pt-4 pb-4">
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-3">
                            <AlertIcon className="h-4 w-4 text-muted-foreground" />
                            <div>
                              <div className="font-medium">
                                {formatAlertDescription(alert)}
                              </div>
                              <div className="text-xs text-muted-foreground">
                                Created {new Date(alert.created_at).toLocaleDateString()}
                                {alert.last_triggered_at && (
                                  <span> • Fired {new Date(alert.last_triggered_at * 1000).toLocaleDateString()}</span>
                                )}
                              </div>
                            </div>
                          </div>

                          <div className="flex items-center gap-2">
                            <Badge variant={status.variant} className="gap-1">
                              <status.icon className="h-3 w-3" />
                              {status.label}
                            </Badge>

                            <div className="flex gap-1">
                              {!alert.is_active && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  onClick={() => handleReactivateAlert(alert.id)}
                                  className="h-8 w-8 p-0"
                                  title="Reactivate alert"
                                >
                                  <RotateCcw className="h-4 w-4" />
                                </Button>
                              )}
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => handleDeleteAlert(alert.id)}
                                className="h-8 w-8 p-0 text-muted-foreground hover:text-red-600"
                                title="Delete alert"
                              >
                                <Trash2 className="h-4 w-4" />
                              </Button>
                            </div>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  )
                })}
              </div>
            )}
          </div>

          {/* Create Alert Form */}
          {showCreateForm && (
            <Card>
              <CardContent className="pt-4 space-y-4">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-medium">Create New Alert</h3>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowCreateForm(false)}
                    className="h-8 w-8 p-0"
                  >
                    ×
                  </Button>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <div>
                    <Label htmlFor="alert-type">Alert Type</Label>
                    <Select value={alertType} onValueChange={(value) => setAlertType(value as typeof alertType)}>
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {ALERT_TYPE_OPTIONS.map((option) => {
                          const IconComponent = option.icon
                          return (
                            <SelectItem key={option.value} value={option.value}>
                              <div className="flex items-center gap-2">
                                <IconComponent className="h-4 w-4" />
                                <span>{option.label}</span>
                              </div>
                            </SelectItem>
                          )
                        })}
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground mt-1">
                      {ALERT_TYPE_OPTIONS.find(opt => opt.value === alertType)?.description}
                    </p>
                  </div>

                  <div>
                    <Label htmlFor="threshold-amount">Bitcoin Amount</Label>
                    <Input
                      id="threshold-amount"
                      value={thresholdInput}
                      onChange={(e) => setThresholdInput(e.target.value)}
                      placeholder="0.00000000"
                      disabled={isSubmitting}
                      className="font-mono"
                    />
                    <p className="text-xs text-muted-foreground mt-1">
                      Enter amount in BTC (e.g., 0.00100000)
                    </p>
                  </div>
                </div>

                <div className="flex justify-end gap-2">
                  <Button
                    variant="outline"
                    onClick={() => setShowCreateForm(false)}
                    disabled={isSubmitting}
                  >
                    Cancel
                  </Button>
                  <Button
                    onClick={handleCreateAlert}
                    disabled={isSubmitting || !thresholdInput.trim()}
                  >
                    {isSubmitting ? "Creating..." : "Create Alert"}
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
