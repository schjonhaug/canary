"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { useParams, useRouter } from "next/navigation"
import {
  Bell,
  Mail,
  MessageCircle,
  MoreHorizontal,
  Pencil,
  Plus,
  Save,
  Target,
  Trash2,
  TrendingDown,
  TrendingUp,
} from "lucide-react"
import {
  EmailProviderFields,
  NtfyProviderFields,
  SmsProviderFields,
} from "@/components/contact-modal/index"
import { PlansModal } from "@/components/plans-modal"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
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
import { useEmailVerification } from "@/hooks/useEmailVerification"
import { useNtfyServerTarget } from "@/hooks/useNtfyServerUrl"
import { usePhonePlaceholder } from "@/hooks/usePhonePlaceholder"
import { useSmsVerification } from "@/hooks/useSmsVerification"
import { api, ApiError } from "@/lib/api"
import {
  btcToSats,
  getTranslatedApiError,
  hasReachedContactLimit,
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

const DEFAULT_NEW_CONTACT_TX_SETTINGS: Omit<ContactDraft, "name" | "methods"> = {
  notify_sending: true,
  notify_sent: true,
  notify_receiving: true,
  notify_received: true,
  notify_cpfp: false,
  notify_rbf: false,
  include_wallet_balance_in_tx_notifications: false,
}

const PROVIDERS = [
  { value: "email", label: "Email", icon: Mail },
  { value: "sms", label: "SMS", icon: MessageCircle },
  { value: "ntfy", label: "ntfy", icon: Bell },
] as const

const THRESHOLD_TYPES = [
  { value: "above", label: "Above", icon: TrendingUp },
  { value: "equals", label: "Equal", icon: Target },
  { value: "below", label: "Below", icon: TrendingDown },
] as const

const EVENT_GROUPS = [
  {
    label: "Activity",
    options: [
      {
        key: "notify_sending",
        label: "Sending",
        description: "An outgoing transaction is seen in the mempool and is still unconfirmed.",
      },
      {
        key: "notify_receiving",
        label: "Receiving",
        description: "An incoming transaction is seen in the mempool and is still unconfirmed.",
      },
    ],
  },
  {
    label: "First confirmation",
    options: [
      {
        key: "notify_sent",
        label: "Sent",
        description: "An outgoing transaction receives its first confirmation.",
      },
      {
        key: "notify_received",
        label: "Received",
        description: "An incoming transaction receives its first confirmation.",
      },
    ],
  },
  {
    label: "Replacements / fee bumps",
    options: [
      {
        key: "notify_rbf",
        label: "RBF replacement",
        description: "Replace-By-Fee: an unconfirmed transaction is replaced by a newer version.",
      },
      {
        key: "notify_cpfp",
        label: "CPFP fee bump",
        description: "Child Pays For Parent: a child transaction is used to help confirm its parent.",
      },
    ],
  },
] as const

const TX_NOTIFICATION_KEYS = [
  "notify_sending",
  "notify_sent",
  "notify_receiving",
  "notify_received",
  "notify_cpfp",
  "notify_rbf",
] as const

type TxNotificationKey = (typeof TX_NOTIFICATION_KEYS)[number]

function sanitizeForNtfyTopic(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .substring(0, 30)
}

function generateDefaultNtfyTopic(name: string, walletChecksum: string): string {
  const sanitizedName = sanitizeForNtfyTopic(name)
  if (!sanitizedName) return walletChecksum.substring(0, 8)
  return `${sanitizedName}-${walletChecksum.substring(0, 8)}`
}

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

function hasSelectedTxNotifications(draft: Pick<ContactDraft, TxNotificationKey>) {
  return TX_NOTIFICATION_KEYS.some((key) => draft[key])
}

function txSettingsFromDraft(draft: ContactDraft) {
  return {
    notify_sending: draft.notify_sending,
    notify_sent: draft.notify_sent,
    notify_receiving: draft.notify_receiving,
    notify_received: draft.notify_received,
    notify_cpfp: draft.notify_cpfp,
    notify_rbf: draft.notify_rbf,
    include_wallet_balance_in_tx_notifications:
      draft.include_wallet_balance_in_tx_notifications,
  }
}

function nullableThresholdFieldMatches<T>(
  left: T | null | undefined,
  right: T | null | undefined
) {
  return left == null ? right == null : left === right
}

function isMigratedWalletLevelAlert(
  walletLevelAlert: BalanceAlert,
  candidate: BalanceAlert
) {
  return (
    !walletLevelAlert.contact_id &&
    Boolean(candidate.contact_id) &&
    candidate.wallet_checksum === walletLevelAlert.wallet_checksum &&
    candidate.threshold_sats === walletLevelAlert.threshold_sats &&
    candidate.alert_type === walletLevelAlert.alert_type &&
    candidate.created_at === walletLevelAlert.created_at &&
    nullableThresholdFieldMatches(
      candidate.threshold_currency,
      walletLevelAlert.threshold_currency
    ) &&
    nullableThresholdFieldMatches(
      candidate.threshold_fiat_amount,
      walletLevelAlert.threshold_fiat_amount
    )
  )
}

function TransactionEventGroups({
  groups,
  draft,
  onChange,
}: {
  groups: readonly {
    label: string
    options: readonly {
      key: TxNotificationKey
      label: string
      description: string
    }[]
  }[]
  draft: Pick<ContactDraft, TxNotificationKey>
  onChange: (key: TxNotificationKey, checked: boolean) => void
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-3">
      {groups.map((group) => (
        <div key={group.label} className="space-y-3">
          <h4 className="text-xs font-semibold uppercase tracking-normal text-muted-foreground">
            {group.label}
          </h4>
          <div className="space-y-3">
            {group.options.map(({ key, label, description }) => (
              <label key={key} className="flex items-start gap-2 text-sm">
                <Checkbox
                  checked={draft[key]}
                  onCheckedChange={(checked) => onChange(key, checked === true)}
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
        </div>
      ))}
    </div>
  )
}

function NewContactWizardCard({
  walletChecksum,
  isSelfHostedMode,
  onCancel,
  onCreated,
}: {
  walletChecksum: string
  isSelfHostedMode: boolean
  onCancel: () => void
  onCreated: () => void
}) {
  const tCommon = useTranslations("common")
  const tApiErrors = useTranslations("errors.api")
  const phonePlaceholder = usePhonePlaceholder()
  const ntfyServerTarget = useNtfyServerTarget()
  const [step, setStep] = useState(0)
  const [name, setName] = useState("")
  const [providerType, setProviderType] = useState<MethodDraft["provider_type"]>(
    isSelfHostedMode ? "ntfy" : "email"
  )
  const [target, setTarget] = useState("")
  const [ntfyTopic, setNtfyTopic] = useState("")
  const [userEditedNtfyTopic, setUserEditedNtfyTopic] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [isCreating, setIsCreating] = useState(false)

  const smsVerification = useSmsVerification({
    walletChecksum,
    contactName: name,
    originalPhoneNumber: null,
    onError: setError,
  })
  const emailVerification = useEmailVerification({
    walletChecksum,
    contactName: name,
    originalEmailAddress: null,
    onError: setError,
  })

  const availableProviders = useMemo(() => {
    if (isSelfHostedMode) return PROVIDERS.filter((provider) => provider.value === "ntfy")
    return PROVIDERS
  }, [isSelfHostedMode])

  const selectedProvider =
    PROVIDERS.find((provider) => provider.value === providerType) ?? PROVIDERS[0]
  const SelectedProviderIcon = selectedProvider.icon

  useEffect(() => {
    if (providerType === "ntfy" && ntfyServerTarget.defaultTopic && !userEditedNtfyTopic) {
      setNtfyTopic(ntfyServerTarget.defaultTopic)
    }
  }, [providerType, ntfyServerTarget.defaultTopic, userEditedNtfyTopic])

  useEffect(() => {
    if (providerType === "ntfy" && !ntfyServerTarget.defaultTopic && !userEditedNtfyTopic) {
      setNtfyTopic(generateDefaultNtfyTopic(name || "contact", walletChecksum))
    }
  }, [name, providerType, ntfyServerTarget.defaultTopic, userEditedNtfyTopic, walletChecksum])

  const targetValue = providerType === "ntfy" ? ntfyTopic : target
  const providerVerified =
    providerType === "ntfy" ||
    (providerType === "sms" && smsVerification.isVerified) ||
    (providerType === "email" && emailVerification.isVerified)

  const handleProviderChange = (value: string) => {
    setProviderType(value as MethodDraft["provider_type"])
    setTarget("")
    setError(null)
    smsVerification.reset()
    emailVerification.reset()
    if (value === "ntfy" && !ntfyTopic && !userEditedNtfyTopic) {
      setNtfyTopic(ntfyServerTarget.defaultTopic || generateDefaultNtfyTopic(name || "contact", walletChecksum))
    }
  }

  const nextFromName = () => {
    if (!name.trim()) {
      setError("Enter a contact name")
      return
    }
    setError(null)
    setStep(1)
  }

  const validateMethod = () => {
    if (!targetValue.trim()) {
      setError(
        providerType === "ntfy"
          ? "Enter an ntfy topic"
          : providerType === "sms"
            ? "Enter a phone number"
            : "Enter an email address"
      )
      return
    }
    if (!providerVerified) {
      setError(providerType === "sms" ? "Verify the phone number first" : "Verify the email first")
      return
    }
    setError(null)
    return true
  }

  const createContact = async () => {
    if (!name.trim()) {
      setError("Enter a contact name")
      return
    }
    if (!validateMethod()) {
      return
    }

    setIsCreating(true)
    setError(null)
    try {
      await api.createContact(
        walletChecksum,
        name.trim(),
        [
          {
            provider_type: providerType,
            notification_target:
              providerType === "email"
                ? emailVerification.verificationAddress || target.trim()
                : providerType === "sms"
                  ? smsVerification.verificationPhone || target.trim()
                  : ntfyTopic.trim(),
            is_enabled: true,
          },
        ],
        txSettingsFromDraft({
          name: name.trim(),
          methods: [],
          ...DEFAULT_NEW_CONTACT_TX_SETTINGS,
        })
      )
      onCreated()
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : "Failed to create contact")
    } finally {
      setIsCreating(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold">New contact</h2>
          </div>
          <Button variant="ghost" size="sm" onClick={onCancel} disabled={isCreating}>
            Cancel
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-6">
        <section className="space-y-3">
          <h3 className="text-sm font-medium">Name</h3>
          <Input
            value={name}
            onChange={(event) => {
              const nextName = event.target.value
              setName(nextName)
              if (providerType === "ntfy" && !ntfyServerTarget.defaultTopic && !userEditedNtfyTopic) {
                setNtfyTopic(generateDefaultNtfyTopic(nextName || "contact", walletChecksum))
              }
            }}
            placeholder="Alice"
            disabled={isCreating}
            aria-label="New contact name"
          />
          {step === 0 && (
            <div className="flex justify-end">
              <Button onClick={nextFromName} disabled={isCreating}>
                Next
              </Button>
            </div>
          )}
        </section>

        {step >= 1 && (
          <section className="space-y-3">
            <h3 className="text-sm font-medium">Delivery method</h3>
            <div className="rounded-md border p-3">
              <div className="mb-3 flex flex-wrap items-center gap-3">
                <div className="flex items-center gap-2 text-sm font-medium">
                  <SelectedProviderIcon className="h-4 w-4" />
                  {selectedProvider.label}
                </div>
                {availableProviders.length > 1 && (
                  <Select value={providerType} onValueChange={handleProviderChange}>
                    <SelectTrigger className="w-40" aria-label="Delivery method">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {availableProviders.map((provider) => (
                        <SelectItem key={provider.value} value={provider.value}>
                          {provider.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              </div>
              {providerType === "ntfy" && (
                <NtfyProviderFields
                  topic={ntfyTopic}
                  onTopicChange={(value) => {
                    setNtfyTopic(value)
                    setUserEditedNtfyTopic(true)
                  }}
                  defaultTopicPlaceholder={
                    ntfyServerTarget.defaultTopic ||
                    generateDefaultNtfyTopic(name || "contact", walletChecksum)
                  }
                  disabled={isCreating}
                  ntfyServerUrl={ntfyServerTarget.url}
                  ntfyServerIsBrowserSafe={ntfyServerTarget.isBrowserSafe}
                />
              )}
              {providerType === "sms" && (
                <SmsProviderFields
                  phoneNumber={target}
                  onPhoneNumberChange={(value) => {
                    setTarget(value)
                    setError(null)
                    smsVerification.clearPhoneError()
                    if (
                      smsVerification.isVerified ||
                      (smsVerification.verificationPhone &&
                        value.trim() !== smsVerification.verificationPhone)
                    ) {
                      smsVerification.reset()
                    }
                  }}
                  phonePlaceholder={phonePlaceholder}
                  phoneError={smsVerification.phoneError}
                  disabled={isCreating}
                  verificationRequired
                  verificationSent={smsVerification.verificationSent}
                  verificationCode={smsVerification.verificationCode}
                  onVerificationCodeChange={(code) => {
                    smsVerification.setVerificationCode(code)
                    smsVerification.clearVerificationError()
                  }}
                  verificationPhone={smsVerification.verificationPhone}
                  verificationError={smsVerification.verificationError}
                  isVerified={smsVerification.isVerified}
                  showSuccess={smsVerification.showSuccess}
                  isSending={smsVerification.isSending}
                  isVerifying={smsVerification.isVerifying}
                  timeRemaining={smsVerification.timeRemaining}
                  formatTime={smsVerification.formatTime}
                  onSendVerification={() => smsVerification.sendVerification(target.trim())}
                  onVerifyCode={() => smsVerification.verifyCode()}
                  onResendCode={() => smsVerification.resendCode()}
                />
              )}
              {providerType === "email" && (
                <EmailProviderFields
                  emailAddress={target}
                  onEmailAddressChange={(value) => {
                    setTarget(value)
                    setError(null)
                    emailVerification.clearEmailError()
                    if (
                      emailVerification.isVerified ||
                      (emailVerification.verificationAddress &&
                        value.trim() !== emailVerification.verificationAddress)
                    ) {
                      emailVerification.reset()
                    }
                  }}
                  emailPlaceholder={tCommon("emailPlaceholder")}
                  emailError={emailVerification.emailError}
                  disabled={isCreating}
                  verificationRequired
                  verificationSent={emailVerification.verificationSent}
                  verificationCode={emailVerification.verificationCode}
                  onVerificationCodeChange={(code) => {
                    emailVerification.setVerificationCode(code)
                    emailVerification.clearVerificationError()
                  }}
                  verificationAddress={emailVerification.verificationAddress}
                  verificationError={emailVerification.verificationError}
                  isVerified={emailVerification.isVerified}
                  showSuccess={emailVerification.showSuccess}
                  isSending={emailVerification.isSending}
                  isVerifying={emailVerification.isVerifying}
                  timeRemaining={emailVerification.timeRemaining}
                  formatTime={emailVerification.formatTime}
                  onSendVerification={() => emailVerification.sendVerification(target.trim())}
                  onVerifyCode={() => emailVerification.verifyCode()}
                  onResendCode={() => emailVerification.resendCode()}
                />
              )}
            </div>
            {step === 1 && (
              <div className="flex justify-end">
                <Button onClick={createContact} disabled={isCreating}>
                  {isCreating ? tCommon("saving") : "Create contact"}
                </Button>
              </div>
            )}
          </section>
        )}

        {error && (
          <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        )}
      </CardContent>
    </Card>
  )
}

function ContactNotificationCard({
  contact,
  alerts,
  walletChecksum,
  isSelfHostedMode,
  preferredFiatCurrency,
  onSaved,
  onDeleted,
}: {
  contact: Contact
  alerts: BalanceAlert[]
  walletChecksum: string
  isSelfHostedMode: boolean
  preferredFiatCurrency: string
  onSaved: () => void
  onDeleted: () => void
}) {
  const tCommon = useTranslations("common")
  const [draft, setDraft] = useState<ContactDraft>(() => contactToDraft(contact))
  const [editDraft, setEditDraft] = useState<ContactDraft>(() => contactToDraft(contact))
  const [contactError, setContactError] = useState<string | null>(null)
  const [thresholdError, setThresholdError] = useState<string | null>(null)
  const [isSaving, setIsSaving] = useState(false)
  const [txSaveState, setTxSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle")
  const [isEditingContact, setIsEditingContact] = useState(false)
  const [thresholdType, setThresholdType] = useState<"below" | "above" | "equals">("below")
  const [thresholdAmount, setThresholdAmount] = useState("")
  const [thresholdCurrency, setThresholdCurrency] = useState("BTC")
  const [newMethodProvider, setNewMethodProvider] =
    useState<MethodDraft["provider_type"]>("ntfy")
  const autosaveQueueRef = useRef(Promise.resolve())
  const autosaveRequestIdRef = useRef(0)
  const latestTxDraftRef = useRef(draft)
  const latestContactDraftRef = useRef(editDraft)
  const isDeletingRef = useRef(false)

  useEffect(() => {
    const nextDraft = contactToDraft(contact)
    setDraft(nextDraft)
    setEditDraft(nextDraft)
    latestTxDraftRef.current = nextDraft
    latestContactDraftRef.current = nextDraft
    setIsEditingContact(false)
  }, [contact])

  useEffect(() => {
    latestContactDraftRef.current = editDraft
  }, [editDraft])

  const availableProviders = useMemo(() => {
    if (isSelfHostedMode) {
      return PROVIDERS.filter((provider) => provider.value === "ntfy")
    }
    return PROVIDERS
  }, [isSelfHostedMode])

  const addableProviders = useMemo(() => {
    const editableProviders = availableProviders.filter((provider) => provider.value === "ntfy")

    if (editableProviders.length !== 1) {
      return editableProviders
    }

    const onlyProvider = editableProviders[0]
    const hasOnlyProvider = editDraft.methods.some(
      (method) => method.provider_type === onlyProvider.value
    )

    return hasOnlyProvider ? [] : editableProviders
  }, [availableProviders, editDraft.methods])

  const providerToAdd =
    addableProviders.find((provider) => provider.value === newMethodProvider) ??
    addableProviders[0]
  const hasTxNotifications = hasSelectedTxNotifications(draft)
  const hasSingleEditableDeliveryMethod = editDraft.methods.length === 1
  const fiatThresholdCurrency = preferredFiatCurrency || "USD"
  const deliverySummary =
    draft.methods.length === 0
      ? "No delivery methods"
      : draft.methods
          .map((method) => {
            const provider = PROVIDERS.find((item) => item.value === method.provider_type)
            const target = method.notification_target.trim()
            return `${provider?.label ?? method.provider_type}: ${target || "not set"}`
          })
          .join(", ")

  const updateContactWithDraft = async (
    nextTxDraft: ContactDraft,
    nextContactDraft = latestContactDraftRef.current
  ) => {
    await api.updateContact(
      walletChecksum,
      contact.id,
      nextContactDraft.name.trim(),
      nextContactDraft.methods
        .filter((method) => method.notification_target.trim())
        .map((method) => ({
          provider_type: method.provider_type,
          notification_target: method.notification_target.trim(),
          is_enabled: method.is_enabled,
        })),
      txSettingsFromDraft(nextTxDraft)
    )
  }

  const saveContact = async () => {
    const nextContactDraft = editDraft
    latestContactDraftRef.current = nextContactDraft
    setIsSaving(true)
    setContactError(null)
    try {
      const saveRequest = autosaveQueueRef.current
        .catch(() => undefined)
        .then(() =>
          updateContactWithDraft(latestTxDraftRef.current, nextContactDraft)
        )

      autosaveQueueRef.current = saveRequest.then(
        () => undefined,
        () => undefined
      )

      await saveRequest
      const savedDraft = {
        ...latestTxDraftRef.current,
        name: nextContactDraft.name,
        methods: nextContactDraft.methods,
      }
      setDraft(savedDraft)
      setEditDraft(savedDraft)
      latestTxDraftRef.current = savedDraft
      latestContactDraftRef.current = savedDraft
      setIsEditingContact(false)
      onSaved()
    } catch (err) {
      setContactError(err instanceof Error ? err.message : "Failed to save contact")
    } finally {
      setIsSaving(false)
    }
  }

  const cancelContactEdit = () => {
    setEditDraft(draft)
    latestContactDraftRef.current = draft
    setContactError(null)
    setIsEditingContact(false)
  }

  const autosaveTxDraft = (nextDraft: ContactDraft) => {
    if (isDeletingRef.current) return

    const requestId = autosaveRequestIdRef.current + 1
    autosaveRequestIdRef.current = requestId
    setDraft(nextDraft)
    latestTxDraftRef.current = nextDraft
    setTxSaveState("saving")
    setContactError(null)

    const saveRequest = autosaveQueueRef.current
      .catch(() => undefined)
      .then(() => updateContactWithDraft(nextDraft))

    autosaveQueueRef.current = saveRequest.then(
      () => undefined,
      () => undefined
    )

    saveRequest
      .then(() => {
        if (requestId === autosaveRequestIdRef.current) {
          setTxSaveState("saved")
        }
      })
      .catch((err) => {
        if (requestId === autosaveRequestIdRef.current) {
          setTxSaveState("error")
          setContactError(err instanceof Error ? err.message : "Failed to save notification settings")
        }
      })
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
    isDeletingRef.current = true
    autosaveRequestIdRef.current += 1
    try {
      await autosaveQueueRef.current.catch(() => undefined)
      await api.deleteContact(walletChecksum, contact.id)
      onDeleted()
    } catch (err) {
      isDeletingRef.current = false
      setContactError(err instanceof Error ? err.message : "Failed to delete contact")
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0 space-y-1">
            <h2 className="truncate text-base font-semibold">{draft.name}</h2>
            <p className="truncate text-sm text-muted-foreground">{deliverySummary}</p>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon" aria-label="Contact actions">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => setIsEditingContact(true)}>
                <Pencil className="mr-2 h-4 w-4" />
                Edit contact
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={deleteContact}
                className="text-destructive focus:text-destructive"
              >
                <Trash2 className="mr-2 h-4 w-4" />
                Delete contact
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </CardHeader>
      <CardContent className="space-y-6">
        {contactError && <p className="text-sm text-destructive">{contactError}</p>}

        {isEditingContact && (
          <section className="space-y-3 rounded-md border p-3">
          <h3 className="text-sm font-medium text-muted-foreground">Contact</h3>
          <Input
            value={editDraft.name}
            onChange={(event) =>
              setEditDraft((prev) => ({ ...prev, name: event.target.value }))
            }
            className="max-w-sm font-medium"
            aria-label="Contact name"
          />
          <div className="space-y-2">
            {editDraft.methods.map((method, index) => {
              const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
              const Icon = provider.icon
              const canEditTarget = method.provider_type === "ntfy"
              return (
                <div
                  key={`${method.provider_type}-${index}`}
                  className={
                    hasSingleEditableDeliveryMethod
                      ? "grid gap-2 sm:grid-cols-[120px_1fr]"
                      : "grid gap-2 sm:grid-cols-[120px_1fr_auto]"
                  }
                >
                  <div className="flex items-center gap-2 text-sm">
                    {!hasSingleEditableDeliveryMethod && (
                      <Checkbox
                        checked={method.is_enabled}
                        onCheckedChange={(checked) =>
                          setEditDraft((prev) => ({
                            ...prev,
                            methods: prev.methods.map((item, methodIndex) =>
                              methodIndex === index ? { ...item, is_enabled: checked === true } : item
                            ),
                          }))
                        }
                      />
                    )}
                    <Icon className="h-4 w-4" />
                    {provider.label}
                  </div>
                  <Input
                    value={method.notification_target}
                    placeholder={methodPlaceholder(method.provider_type)}
                    readOnly={!canEditTarget}
                    disabled={!canEditTarget}
                    onChange={(event) =>
                      setEditDraft((prev) => ({
                        ...prev,
                        methods: prev.methods.map((item, methodIndex) =>
                          methodIndex === index
                            ? { ...item, notification_target: event.target.value }
                            : item
                        ),
                      }))
                    }
                  />
                  {!hasSingleEditableDeliveryMethod && (
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() =>
                        setEditDraft((prev) => ({
                          ...prev,
                          methods: prev.methods.filter((_, methodIndex) => methodIndex !== index),
                        }))
                      }
                      aria-label="Delete delivery method"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  )}
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
                  setEditDraft((prev) => ({
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
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={cancelContactEdit} disabled={isSaving}>
              Cancel
            </Button>
            <Button onClick={saveContact} disabled={isSaving || !editDraft.name.trim()} size="sm">
              <Save className="h-4 w-4" />
              {isSaving ? tCommon("saving") : "Save contact"}
            </Button>
          </div>
          </section>
        )}

        <section className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <h3 className="text-sm font-medium text-muted-foreground">Transaction notifications</h3>
            <span className="text-xs text-muted-foreground">
              {txSaveState === "saving"
                ? "Saving..."
                : txSaveState === "saved"
                  ? "Saved"
                  : txSaveState === "error"
                    ? "Could not save"
                    : "Saved on change"}
            </span>
          </div>
          <div>
            <label className="flex items-start gap-2 text-sm">
              <Checkbox
                checked={
                  hasTxNotifications &&
                  draft.include_wallet_balance_in_tx_notifications
                }
                disabled={!hasTxNotifications}
                onCheckedChange={(checked) =>
                  autosaveTxDraft({
                    ...draft,
                    include_wallet_balance_in_tx_notifications: checked === true,
                  })
                }
              />
              <span className="space-y-1">
                <span className="block font-medium">
                  Include wallet balance in transaction notifications
                </span>
                <span className="block text-xs leading-snug text-muted-foreground">
                  {hasTxNotifications
                    ? "Applies to all selected transaction notification types below."
                    : "Select at least one transaction notification type first."}
                </span>
              </span>
            </label>
          </div>
          <TransactionEventGroups
            groups={EVENT_GROUPS}
            draft={draft}
            onChange={(key, checked) => autosaveTxDraft({ ...draft, [key]: checked })}
          />
        </section>

        <section className="space-y-3">
          <h3 className="text-sm font-medium text-muted-foreground">Balance threshold notifications</h3>
          {alerts.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {alerts.map((alert) => (
                <div key={alert.id} className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm">
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
                    className="h-7 w-7"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <RadioGroup
              value={thresholdType}
              onValueChange={(value) => {
                setThresholdType(value as typeof thresholdType)
                setThresholdError(null)
              }}
              className="flex flex-wrap items-center gap-4"
              aria-label="Threshold type"
            >
              {THRESHOLD_TYPES.map((type) => {
                const Icon = type.icon
                return (
                  <label
                    key={type.value}
                    className="flex items-center gap-2 text-sm"
                    htmlFor={`threshold-${contact.id}-${type.value}`}
                  >
                    <RadioGroupItem
                      value={type.value}
                      id={`threshold-${contact.id}-${type.value}`}
                    />
                    <Icon className="h-4 w-4 text-muted-foreground" />
                    {type.label}
                  </label>
                )
              })}
            </RadioGroup>
            <Input
              value={thresholdAmount}
              onChange={(event) => {
                setThresholdAmount(event.target.value)
                setThresholdError(null)
              }}
              placeholder={thresholdCurrency === "BTC" ? "0.10" : "10000"}
              className="w-[120px]"
            />
            <Select
              value={thresholdCurrency}
              onValueChange={(value) => {
                setThresholdCurrency(value)
                setThresholdError(null)
              }}
            >
              <SelectTrigger className="w-[120px]" aria-label="Threshold currency">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="BTC">BTC</SelectItem>
                <SelectItem value={fiatThresholdCurrency}>{fiatThresholdCurrency}</SelectItem>
              </SelectContent>
            </Select>
            <Button
              onClick={addThreshold}
              disabled={!thresholdAmount.trim()}
              className="w-[160px] whitespace-nowrap"
            >
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
    user,
    billingStatus,
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
  const [isCreatingContact, setIsCreatingContact] = useState(false)
  const [showUpgradeModal, setShowUpgradeModal] = useState(false)
  const [preferredFiatCurrency, setPreferredFiatCurrency] = useState("USD")

  const load = async () => {
    setIsLoading(true)
    setError(null)
    try {
      const [data, preferences] = await Promise.all([
        api.getWalletNotifications(checksum),
        api.getUserPreferences().catch(() => null),
      ])
      setWallet(data.wallet)
      setContacts(data.contacts)
      setAlerts(data.balance_alerts)
      if (preferences?.preferred_fiat_currency) {
        setPreferredFiatCurrency(preferences.preferred_fiat_currency)
      }
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
  const walletLevelAlerts = useMemo(
    () =>
      alerts.filter(
        (alert) =>
          !alert.contact_id &&
          (alert.is_active ||
            !alerts.some((candidate) =>
              isMigratedWalletLevelAlert(alert, candidate)
            ))
      ),
    [alerts]
  )

  const sortedContacts = useMemo(() => {
    return [...contacts].sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" })
    )
  }, [contacts])

  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || "personal"
  const contactLimitReached =
    isCloudMode && hasReachedContactLimit(contacts.length, currentTier)

  const startContactCreation = () => {
    if (contactLimitReached) {
      setShowUpgradeModal(true)
      return
    }
    setIsCreatingContact(true)
  }

  const deleteWalletLevelAlert = async (alertId: string) => {
    setError(null)
    try {
      await api.deleteBalanceAlert(alertId)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete threshold")
    }
  }

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
          {!isCreatingContact && (
            <Button onClick={startContactCreation}>
              <Plus className="h-4 w-4" />
              Add contact
            </Button>
          )}
        </div>

        <div className="space-y-4">
          {isCreatingContact && (
            <NewContactWizardCard
              walletChecksum={wallet!.checksum}
              isSelfHostedMode={isSelfHostedMode}
              onCancel={() => setIsCreatingContact(false)}
              onCreated={() => {
                setIsCreatingContact(false)
                load()
              }}
            />
          )}

          {walletLevelAlerts.length > 0 && (
            <Card>
              <CardHeader>
                <h2 className="text-base font-semibold">
                  Legacy wallet balance thresholds
                </h2>
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap gap-2">
                  {walletLevelAlerts.map((alert) => (
                    <div
                      key={alert.id}
                      className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm"
                    >
                      <span>
                        {alert.alert_type}{" "}
                        {alert.threshold_currency && alert.threshold_fiat_amount
                          ? `${alert.threshold_fiat_amount} ${alert.threshold_currency}`
                          : `${satsToBtc(alert.threshold_sats)} BTC`}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => deleteWalletLevelAlert(alert.id)}
                        aria-label="Delete wallet-level threshold"
                        className="h-7 w-7"
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {contacts.length === 0 && !isCreatingContact ? (
            <Card>
              <CardContent className="py-8 text-center text-sm text-muted-foreground">
                No contacts added yet
              </CardContent>
            </Card>
          ) : (
            <>
              {sortedContacts.map((contact) => (
                <ContactNotificationCard
                  key={contact.id}
                  contact={contact}
                  alerts={alertsByContact[contact.id] || []}
                  walletChecksum={wallet!.checksum}
                  isSelfHostedMode={isSelfHostedMode}
                  preferredFiatCurrency={preferredFiatCurrency}
                  onSaved={load}
                  onDeleted={load}
                />
              ))}
            </>
          )}
        </div>
      </section>
      <PlansModal
        isOpen={showUpgradeModal}
        onClose={() => setShowUpgradeModal(false)}
        currentTier={currentTier}
        currentContactCount={contacts.length}
        limitType="contacts"
        isTrialUser={billingStatus?.subscription_status === "trialing"}
        billingStatus={billingStatus ? {
          subscription_status: billingStatus.subscription_status,
          stripe_customer_id: billingStatus.stripe_customer_id,
        } : undefined}
      />
    </div>
  )
}
