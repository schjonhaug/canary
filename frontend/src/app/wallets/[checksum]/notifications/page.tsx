"use client"

import { useEffect, useMemo, useState } from "react"
import { useParams, useRouter } from "next/navigation"
import { Bell, Mail, MessageCircle, Plus, Save, Trash2 } from "lucide-react"
import { ContactModal } from "@/components/contact-modal"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  WalletDetailHeader,
  WalletDetailSkeleton,
  getWalletDetailErrorState,
} from "@/components/wallet-detail"
import { useAuth } from "@/contexts/auth-context"
import { useWalletsContext } from "@/contexts/wallets-context"
import { api, ApiError } from "@/lib/api"
import {
  btcToSats,
  getTranslatedApiError,
  parseBtcInput,
  satsToBtc,
} from "@/lib/utils"
import { useTranslations } from "next-intl"
import type { BalanceAlert, Contact, Wallet } from "@/types"

type MethodDraft = {
  provider_type: "email" | "sms" | "ntfy"
  notification_target: string
  is_enabled: boolean
}

type ContactDraft = {
  name: string
  methods: MethodDraft[]
  notify_sending: boolean
  notify_sent: boolean
  notify_receiving: boolean
  notify_received: boolean
  notify_cpfp: boolean
  notify_rbf: boolean
  include_wallet_balance_in_tx_notifications: boolean
}

const PROVIDERS = [
  { value: "email", label: "Email", icon: Mail },
  { value: "sms", label: "SMS", icon: MessageCircle },
  { value: "ntfy", label: "ntfy", icon: Bell },
] as const

const EVENT_OPTIONS = [
  {
    key: "notify_sending",
    label: "Sending",
    description: "An outgoing transaction is seen in the mempool and is still unconfirmed.",
  },
  {
    key: "notify_sent",
    label: "Sent",
    description: "An outgoing transaction receives its first confirmation.",
  },
  {
    key: "notify_receiving",
    label: "Receiving",
    description: "An incoming transaction is seen in the mempool and is still unconfirmed.",
  },
  {
    key: "notify_received",
    label: "Received",
    description: "An incoming transaction receives its first confirmation.",
  },
  {
    key: "notify_cpfp",
    label: "CPFP fee bump",
    description: "Child Pays For Parent: a child transaction is used to help confirm its parent.",
  },
  {
    key: "notify_rbf",
    label: "RBF replacement",
    description: "Replace-By-Fee: an unconfirmed transaction is replaced by a newer version.",
  },
] as const

function contactToDraft(contact: Contact): ContactDraft {
  return {
    name: contact.name,
    methods: contact.notification_methods.map((method) => ({
      provider_type: method.provider_type,
      notification_target: method.notification_target,
      is_enabled: method.is_enabled ?? true,
    })),
    notify_sending: contact.notify_sending ?? true,
    notify_sent: contact.notify_sent ?? true,
    notify_receiving: contact.notify_receiving ?? true,
    notify_received: contact.notify_received ?? true,
    notify_cpfp: contact.notify_cpfp ?? true,
    notify_rbf: contact.notify_rbf ?? true,
    include_wallet_balance_in_tx_notifications:
      contact.include_wallet_balance_in_tx_notifications ?? false,
  }
}

function methodPlaceholder(providerType: MethodDraft["provider_type"]) {
  if (providerType === "email") return "alice@example.com"
  if (providerType === "sms") return "+47 123 45 678"
  return "canary-topic"
}

function ContactNotificationCard({
  contact,
  alerts,
  walletChecksum,
  isSelfHostedMode,
  onSaved,
  onDeleted,
}: {
  contact: Contact
  alerts: BalanceAlert[]
  walletChecksum: string
  isSelfHostedMode: boolean
  onSaved: () => void
  onDeleted: () => void
}) {
  const tCommon = useTranslations("common")
  const [draft, setDraft] = useState<ContactDraft>(() => contactToDraft(contact))
  const [contactError, setContactError] = useState<string | null>(null)
  const [thresholdError, setThresholdError] = useState<string | null>(null)
  const [isSaving, setIsSaving] = useState(false)
  const [thresholdType, setThresholdType] = useState<"below" | "above" | "equals">("below")
  const [thresholdAmount, setThresholdAmount] = useState("")
  const [thresholdCurrency, setThresholdCurrency] = useState<"BTC" | "USD">("BTC")
  const [newMethodProvider, setNewMethodProvider] =
    useState<MethodDraft["provider_type"]>("ntfy")

  useEffect(() => {
    setDraft(contactToDraft(contact))
  }, [contact])

  const availableProviders = useMemo(() => {
    if (isSelfHostedMode) {
      return PROVIDERS.filter((provider) => provider.value === "ntfy")
    }
    return PROVIDERS
  }, [isSelfHostedMode])

  const addableProviders = useMemo(() => {
    if (availableProviders.length !== 1) {
      return availableProviders
    }

    const onlyProvider = availableProviders[0]
    const hasOnlyProvider = draft.methods.some(
      (method) => method.provider_type === onlyProvider.value
    )

    return hasOnlyProvider ? [] : availableProviders
  }, [availableProviders, draft.methods])

  const providerToAdd =
    addableProviders.find((provider) => provider.value === newMethodProvider) ??
    addableProviders[0]

  const saveContact = async () => {
    setIsSaving(true)
    setContactError(null)
    try {
      await api.updateContact(
        walletChecksum,
        contact.id,
        draft.name.trim(),
        draft.methods
          .filter((method) => method.notification_target.trim())
          .map((method) => ({
            provider_type: method.provider_type,
            notification_target: method.notification_target.trim(),
            is_enabled: method.is_enabled,
          })),
        {
          notify_sending: draft.notify_sending,
          notify_sent: draft.notify_sent,
          notify_receiving: draft.notify_receiving,
          notify_received: draft.notify_received,
          notify_cpfp: draft.notify_cpfp,
          notify_rbf: draft.notify_rbf,
          include_wallet_balance_in_tx_notifications:
            draft.include_wallet_balance_in_tx_notifications,
        }
      )
      onSaved()
    } catch (err) {
      setContactError(err instanceof Error ? err.message : "Failed to save contact")
    } finally {
      setIsSaving(false)
    }
  }

  const addThreshold = async () => {
    setThresholdError(null)
    try {
      if (thresholdCurrency === "BTC") {
        const btc = parseBtcInput(thresholdAmount)
        if (btc === null) {
          setThresholdError("Enter a valid BTC amount")
          return
        }
        await api.createBalanceAlert(walletChecksum, {
          contact_id: contact.id,
          alert_type: thresholdType,
          threshold_sats: btcToSats(btc),
        })
      } else {
        const amount = Number.parseFloat(thresholdAmount)
        if (!Number.isFinite(amount) || amount <= 0) {
          setThresholdError("Enter a valid fiat amount")
          return
        }
        await api.createBalanceAlert(walletChecksum, {
          contact_id: contact.id,
          alert_type: thresholdType,
          threshold_currency: thresholdCurrency,
          threshold_fiat_amount: amount,
        })
      }
      setThresholdAmount("")
      onSaved()
    } catch (err) {
      setThresholdError(err instanceof Error ? err.message : "Failed to add threshold")
    }
  }

  const deleteThreshold = async (alertId: string) => {
    setThresholdError(null)
    try {
      await api.deleteBalanceAlert(alertId)
      onSaved()
    } catch (err) {
      setThresholdError(err instanceof Error ? err.message : "Failed to delete threshold")
    }
  }

  const deleteContact = async () => {
    setContactError(null)
    try {
      await api.deleteContact(walletChecksum, contact.id)
      onDeleted()
    } catch (err) {
      setContactError(err instanceof Error ? err.message : "Failed to delete contact")
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <Input
            value={draft.name}
            onChange={(event) => setDraft((prev) => ({ ...prev, name: event.target.value }))}
            className="max-w-sm font-medium"
            aria-label="Contact name"
          />
          <div className="flex gap-2">
            <Button onClick={saveContact} disabled={isSaving || !draft.name.trim()} size="sm">
              <Save className="h-4 w-4" />
              {isSaving ? tCommon("saving") : tCommon("save")}
            </Button>
            <Button onClick={deleteContact} variant="destructive" size="sm">
              <Trash2 className="h-4 w-4" />
              {tCommon("delete")}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-6">
        {contactError && <p className="text-sm text-destructive">{contactError}</p>}

        <section className="space-y-3">
          <h3 className="text-sm font-medium text-muted-foreground">Delivery methods</h3>
          <div className="space-y-2">
            {draft.methods.map((method, index) => {
              const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
              const Icon = provider.icon
              return (
                <div key={`${method.provider_type}-${index}`} className="grid gap-2 sm:grid-cols-[120px_1fr_auto]">
                  <label className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={method.is_enabled}
                      onCheckedChange={(checked) =>
                        setDraft((prev) => ({
                          ...prev,
                          methods: prev.methods.map((item, methodIndex) =>
                            methodIndex === index ? { ...item, is_enabled: checked === true } : item
                          ),
                        }))
                      }
                    />
                    <Icon className="h-4 w-4" />
                    {provider.label}
                  </label>
                  <Input
                    value={method.notification_target}
                    placeholder={methodPlaceholder(method.provider_type)}
                    onChange={(event) =>
                      setDraft((prev) => ({
                        ...prev,
                        methods: prev.methods.map((item, methodIndex) =>
                          methodIndex === index
                            ? { ...item, notification_target: event.target.value }
                            : item
                        ),
                      }))
                    }
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() =>
                      setDraft((prev) => ({
                        ...prev,
                        methods: prev.methods.filter((_, methodIndex) => methodIndex !== index),
                      }))
                    }
                    aria-label="Delete delivery method"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              )
            })}
          </div>
          {providerToAdd && (
            <div className="flex flex-wrap items-center gap-2">
              {addableProviders.length > 1 && (
                <Select
                  value={newMethodProvider}
                  onValueChange={(value) =>
                    setNewMethodProvider(value as MethodDraft["provider_type"])
                  }
                >
                  <SelectTrigger className="w-36" aria-label="Delivery method type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {addableProviders.map((provider) => (
                      <SelectItem key={provider.value} value={provider.value}>
                        {provider.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  setDraft((prev) => ({
                    ...prev,
                    methods: [
                      ...prev.methods,
                      {
                        provider_type: providerToAdd.value,
                        notification_target: "",
                        is_enabled: true,
                      },
                    ],
                  }))
                }
              >
                <Plus className="h-4 w-4" />
                Add delivery method
              </Button>
            </div>
          )}
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-medium text-muted-foreground">Transaction notifications</h3>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {EVENT_OPTIONS.map(({ key, label, description }) => (
              <label key={key} className="flex items-start gap-2 text-sm">
                <Checkbox
                  checked={draft[key]}
                  onCheckedChange={(checked) =>
                    setDraft((prev) => ({ ...prev, [key]: checked === true }))
                  }
                />
                <span className="space-y-1">
                  <span className="block font-medium leading-none">{label}</span>
                  <span className="block text-xs leading-snug text-muted-foreground">
                    {description}
                  </span>
                </span>
              </label>
            ))}
          </div>
        </section>

        <section className="rounded-md border bg-muted/30 p-3">
          <label className="flex items-start gap-2 text-sm">
            <Checkbox
              checked={draft.include_wallet_balance_in_tx_notifications}
              onCheckedChange={(checked) =>
                setDraft((prev) => ({
                  ...prev,
                  include_wallet_balance_in_tx_notifications: checked === true,
                }))
              }
            />
            <span className="space-y-1">
              <span className="block font-medium">
                Include wallet balance in transaction notifications
              </span>
              <span className="block text-xs leading-snug text-muted-foreground">
                Applies to all selected transaction notification types above.
              </span>
            </span>
          </label>
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-medium text-muted-foreground">Balance threshold notifications</h3>
          {alerts.length === 0 ? (
            <p className="text-sm text-muted-foreground">No thresholds</p>
          ) : (
            <div className="space-y-2">
              {alerts.map((alert) => (
                <div key={alert.id} className="flex items-center justify-between gap-3 rounded-md border p-2 text-sm">
                  <span>
                    {alert.alert_type}{" "}
                    {alert.threshold_currency && alert.threshold_fiat_amount
                      ? `${alert.threshold_fiat_amount} ${alert.threshold_currency}`
                      : `${satsToBtc(alert.threshold_sats)} BTC`}
                  </span>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => deleteThreshold(alert.id)}
                    aria-label="Delete threshold"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          )}
          <div className="grid gap-2 sm:grid-cols-[140px_1fr_120px_auto]">
            <Select
              value={thresholdType}
              onValueChange={(value) => {
                setThresholdType(value as typeof thresholdType)
                setThresholdError(null)
              }}
            >
              <SelectTrigger aria-label="Threshold type">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="below">Below</SelectItem>
                <SelectItem value="above">Above</SelectItem>
                <SelectItem value="equals">Equals</SelectItem>
              </SelectContent>
            </Select>
            <Input
              value={thresholdAmount}
              onChange={(event) => {
                setThresholdAmount(event.target.value)
                setThresholdError(null)
              }}
              placeholder={thresholdCurrency === "BTC" ? "0.10" : "10000"}
            />
            <Select
              value={thresholdCurrency}
              onValueChange={(value) => {
                setThresholdCurrency(value as typeof thresholdCurrency)
                setThresholdError(null)
              }}
            >
              <SelectTrigger aria-label="Threshold currency">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="BTC">BTC</SelectItem>
                <SelectItem value="USD">USD</SelectItem>
              </SelectContent>
            </Select>
            <Button onClick={addThreshold} disabled={!thresholdAmount.trim()}>
              <Plus className="h-4 w-4" />
              Add
            </Button>
          </div>
          {thresholdError && (
            <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {thresholdError}
            </p>
          )}
        </section>
      </CardContent>
    </Card>
  )
}

export default function WalletNotificationsPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  const {
    isAuthenticated,
    isLoading: authLoading,
    isCloudMode,
    isSelfHostedMode,
  } = useAuth()
  const { setCurrentWallet } = useWalletsContext()
  const t = useTranslations("wallets")
  const tCommon = useTranslations("common")
  const tApiErrors = useTranslations("errors.api")
  const [wallet, setWallet] = useState<Wallet | null>(null)
  const [contacts, setContacts] = useState<Contact[]>([])
  const [alerts, setAlerts] = useState<BalanceAlert[]>([])
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [isAddContactOpen, setIsAddContactOpen] = useState(false)

  const load = async () => {
    setIsLoading(true)
    setError(null)
    try {
      const data = await api.getWalletNotifications(checksum)
      setWallet(data.wallet)
      setContacts(data.contacts)
      setAlerts(data.balance_alerts)
      setCurrentWallet?.(data.wallet)
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : "Failed to load notifications")
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) {
      router.push("/sign-in")
    }
  }, [authLoading, isAuthenticated, isCloudMode, router])

  useEffect(() => {
    if (!authLoading && (!isCloudMode || isAuthenticated)) {
      load()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authLoading, isAuthenticated, isCloudMode, checksum])

  const alertsByContact = useMemo(() => {
    return alerts.reduce<Record<string, BalanceAlert[]>>((acc, alert) => {
      if (!alert.contact_id) return acc
      acc[alert.contact_id] = [...(acc[alert.contact_id] || []), alert]
      return acc
    }, {})
  }, [alerts])

  const sortedContacts = useMemo(() => {
    return [...contacts].sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" })
    )
  }, [contacts])

  if (authLoading || (isLoading && !wallet)) {
    return authLoading ? (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon("loading")}</p>
        </div>
      </div>
    ) : (
      <WalletDetailSkeleton />
    )
  }

  if (isCloudMode && !isAuthenticated) return null

  const errorState = getWalletDetailErrorState({
    error,
    wallet,
    checksum,
    t,
    tCommon,
    canDelete: false,
    now: Date.now(),
  })
  if (errorState) return errorState

  return (
    <div className="space-y-6">
      <WalletDetailHeader
        walletChecksum={wallet!.checksum}
        walletName={wallet!.name}
        onNameUpdated={load}
      />

      <section className="space-y-4">
        <div className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">Notifications</h1>
            <p className="text-sm text-muted-foreground">
              Choose who gets notified, how, and for which wallet events.
            </p>
          </div>
          <Button onClick={() => setIsAddContactOpen(true)}>
            <Plus className="h-4 w-4" />
            Add contact
          </Button>
        </div>

        {contacts.length === 0 ? (
          <Card>
            <CardContent className="py-8 text-center text-sm text-muted-foreground">
              No contacts added yet
            </CardContent>
          </Card>
        ) : (
          <div className="space-y-4">
            {sortedContacts.map((contact) => (
              <ContactNotificationCard
                key={contact.id}
                contact={contact}
                alerts={alertsByContact[contact.id] || []}
                walletChecksum={wallet!.checksum}
                isSelfHostedMode={isSelfHostedMode}
                onSaved={load}
                onDeleted={load}
              />
            ))}
          </div>
        )}
      </section>

      <ContactModal
        isOpen={isAddContactOpen}
        onClose={() => setIsAddContactOpen(false)}
        walletChecksum={wallet!.checksum}
        onContactSaved={() => {
          setIsAddContactOpen(false)
          load()
        }}
      />
    </div>
  )
}
