"use client"

import Image from "next/image"
import { Bell, Check, Loader2, Mail, MessageCircle, RadioTower, Send, Webhook as WebhookIcon } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"

import {
  EmailProviderFields,
  NostrProviderFields,
  NtfyProviderFields,
  SmsProviderFields,
  WebhookProviderFields,
  validateWebhookUrl,
} from "@/components/contact-modal/index"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useEmailVerification } from "@/hooks/useEmailVerification"
import { useNtfyServerTarget } from "@/hooks/useNtfyServerUrl"
import { usePhonePlaceholder } from "@/hooks/usePhonePlaceholder"
import { useSmsVerification } from "@/hooks/useSmsVerification"
import { api } from "@/lib/api"
import type { MethodDraft, NotificationProvider } from "./types"
import { generatePrivateNtfyTopic } from "./utils"

export const PROVIDERS = [
  { value: "email", label: "Email", icon: Mail },
  { value: "sms", label: "SMS", icon: MessageCircle },
  { value: "ntfy", label: "ntfy", icon: Bell, imageSrc: "/images/notifications/ntfy-bw.svg" },
  { value: "nostr", label: "Nostr", icon: RadioTower, imageSrc: "/images/notifications/nostr-bw.svg" },
  { value: "webhook", label: "Webhook", icon: WebhookIcon },
] as const

export function ProviderIcon({ provider }: { provider: (typeof PROVIDERS)[number] }) {
  if ("imageSrc" in provider) {
    return <Image src={provider.imageSrc} alt="" aria-hidden="true" width={16} height={16} className="h-4 w-4 shrink-0 dark:invert" />
  }
  const Icon = provider.icon
  return <Icon className="h-4 w-4" aria-hidden="true" />
}

export function availableProviders(isSelfHostedMode: boolean, registeredProviderNames: string[]) {
  if (isSelfHostedMode) {
    return PROVIDERS.filter((provider) =>
      provider.value === "ntfy" ||
      (provider.value === "nostr" && registeredProviderNames.includes("nostr")) ||
      (provider.value === "webhook" && registeredProviderNames.includes("webhook"))
    )
  }
  return PROVIDERS.filter((provider) => ["email", "sms", "ntfy"].includes(provider.value))
}

export function useDeliveryVerification({
  walletChecksum,
  contactName,
  originalSmsTarget,
  originalEmailTarget,
  onError,
}: {
  walletChecksum: string
  contactName: string
  originalSmsTarget: string | null
  originalEmailTarget: string | null
  onError: (error: string | null) => void
}) {
  const sms = useSmsVerification({
    walletChecksum,
    contactName,
    originalPhoneNumber: originalSmsTarget,
    onError,
  })
  const email = useEmailVerification({
    walletChecksum,
    contactName,
    originalEmailAddress: originalEmailTarget,
    onError,
  })
  return { sms, email }
}

type DeliveryVerification = ReturnType<typeof useDeliveryVerification>

export function isMethodVerified(
  method: MethodDraft,
  verification: DeliveryVerification,
  originalMethod?: MethodDraft
) {
  if (method.provider_type !== "sms" && method.provider_type !== "email") return true
  if (originalMethod?.notification_target.trim() === method.notification_target.trim()) return true
  return method.provider_type === "sms" ? verification.sms.isVerified : verification.email.isVerified
}

export function DeliveryTargetFields({
  method,
  onChange,
  verification,
  originalMethod,
  disabled = false,
}: {
  method: MethodDraft
  onChange: (method: MethodDraft) => void
  verification: DeliveryVerification
  originalMethod?: MethodDraft
  disabled?: boolean
}) {
  const tCommon = useTranslations("common")
  const phonePlaceholder = usePhonePlaceholder()
  const ntfyServerTarget = useNtfyServerTarget()
  const isStoredTarget = originalMethod?.notification_target.trim() === method.notification_target.trim()

  if (method.provider_type === "ntfy") {
    return (
      <NtfyProviderFields
        topic={method.notification_target}
        onTopicChange={(notification_target) => onChange({ ...method, notification_target })}
        defaultTopicPlaceholder={ntfyServerTarget.defaultTopic || generatePrivateNtfyTopic()}
        disabled={disabled}
        ntfyServerUrl={ntfyServerTarget.url}
        ntfyServerIsBrowserSafe={ntfyServerTarget.isBrowserSafe}
        containerClassName="space-y-2"
      />
    )
  }
  if (method.provider_type === "nostr") {
    return (
      <NostrProviderFields
        recipient={method.notification_target}
        onRecipientChange={(notification_target) => onChange({ ...method, notification_target })}
        disabled={disabled}
      />
    )
  }
  if (method.provider_type === "webhook") {
    return (
      <WebhookProviderFields
        url={method.notification_target}
        onUrlChange={(notification_target) => onChange({ ...method, notification_target })}
        disabled={disabled}
        showTest={false}
      />
    )
  }
  if (method.provider_type === "sms") {
    const state = verification.sms
    return (
      <SmsProviderFields
        phoneNumber={method.notification_target}
        onPhoneNumberChange={(notification_target) => {
          onChange({ ...method, notification_target })
          state.clearPhoneError()
          if (state.isVerified || (state.verificationPhone && notification_target.trim() !== state.verificationPhone)) state.reset()
        }}
        phonePlaceholder={phonePlaceholder}
        phoneError={state.phoneError}
        disabled={disabled}
        containerClassName="space-y-3"
        verificationButtonLayout="inline"
        verificationRequired={!isStoredTarget && !state.isVerified}
        verificationSent={state.verificationSent}
        verificationCode={state.verificationCode}
        onVerificationCodeChange={(code) => { state.setVerificationCode(code); state.clearVerificationError() }}
        verificationPhone={state.verificationPhone}
        verificationError={state.verificationError}
        isVerified={Boolean(isStoredTarget) || state.isVerified}
        showSuccess={!isStoredTarget && state.showSuccess}
        isSending={state.isSending}
        isVerifying={state.isVerifying}
        timeRemaining={state.timeRemaining}
        formatTime={state.formatTime}
        onSendVerification={() => state.sendVerification(method.notification_target.trim())}
        onVerifyCode={state.verifyCode}
        onResendCode={state.resendCode}
      />
    )
  }

  const state = verification.email
  return (
    <EmailProviderFields
      emailAddress={method.notification_target}
      onEmailAddressChange={(notification_target) => {
        onChange({ ...method, notification_target })
        state.clearEmailError()
        if (state.isVerified || (state.verificationAddress && notification_target.trim() !== state.verificationAddress)) state.reset()
      }}
      emailPlaceholder={tCommon("emailPlaceholder")}
      emailError={state.emailError}
      disabled={disabled}
      containerClassName="space-y-3"
      verificationButtonLayout="inline"
      verificationRequired={!isStoredTarget && !state.isVerified}
      verificationSent={state.verificationSent}
      verificationCode={state.verificationCode}
      onVerificationCodeChange={(code) => { state.setVerificationCode(code); state.clearVerificationError() }}
      verificationAddress={state.verificationAddress}
      verificationError={state.verificationError}
      isVerified={Boolean(isStoredTarget) || state.isVerified}
      showSuccess={!isStoredTarget && state.showSuccess}
      isSending={state.isSending}
      isVerifying={state.isVerifying}
      timeRemaining={state.timeRemaining}
      formatTime={state.formatTime}
      onSendVerification={() => state.sendVerification(method.notification_target.trim())}
      onVerifyCode={state.verifyCode}
      onResendCode={state.resendCode}
    />
  )
}

export function DeliveryStepFields({
  name,
  onNameChange,
  method,
  onMethodChange,
  isSelfHostedMode,
  registeredProviderNames,
  verification,
  ntfyTopicWasEdited,
  onNtfyTopicWasEditedChange,
  onDirty,
  disabled = false,
}: {
  name: string
  onNameChange: (name: string) => void
  method: MethodDraft
  onMethodChange: (method: MethodDraft) => void
  isSelfHostedMode: boolean
  registeredProviderNames: string[]
  verification: DeliveryVerification
  ntfyTopicWasEdited: boolean
  onNtfyTopicWasEditedChange: (edited: boolean) => void
  onDirty?: () => void
  disabled?: boolean
}) {
  const t = useTranslations("walletNotifications")
  const ntfyServerTarget = useNtfyServerTarget()
  const providers = useMemo(
    () => availableProviders(isSelfHostedMode, registeredProviderNames),
    [isSelfHostedMode, registeredProviderNames]
  )
  useEffect(() => {
    if (
      method.provider_type === "ntfy" &&
      ntfyServerTarget.defaultTopic &&
      method.notification_target !== ntfyServerTarget.defaultTopic &&
      !ntfyTopicWasEdited
    ) {
      onMethodChange({ ...method, notification_target: ntfyServerTarget.defaultTopic })
    }
  }, [method, ntfyServerTarget.defaultTopic, ntfyTopicWasEdited, onMethodChange])

  const setProvider = (provider_type: NotificationProvider) => {
    onDirty?.()
    verification.sms.reset()
    verification.email.reset()
    const nextTarget = provider_type === "ntfy"
      ? ntfyServerTarget.defaultTopic || generatePrivateNtfyTopic()
      : ""
    onNtfyTopicWasEditedChange(false)
    onMethodChange({ ...method, provider_type, notification_target: nextTarget })
  }

  return (
    <div className="space-y-5">
      <div className="space-y-2">
        <Label htmlFor="notification-destination-name">{t("delivery.name")}</Label>
        <Input
          id="notification-destination-name"
          value={name}
          onChange={(event) => onNameChange(event.target.value)}
          placeholder={t("editor.namePlaceholder")}
          disabled={disabled}
        />
        <p className="text-xs text-muted-foreground">{t("delivery.nameHint")}</p>
      </div>
      <div className="space-y-2">
        <Label>{t("delivery.method")}</Label>
        <Select value={method.provider_type} onValueChange={(value) => setProvider(value as NotificationProvider)} disabled={disabled}>
          <SelectTrigger aria-label={t("delivery.method")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {providers.map((provider) => (
              <SelectItem key={provider.value} value={provider.value}>
                <span className="flex items-center gap-2">
                  <ProviderIcon provider={provider} />
                  {provider.label}
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <DeliveryTargetFields
        method={method}
        onChange={(next) => {
          onDirty?.()
          if (next.provider_type === "ntfy" && next.notification_target !== method.notification_target) {
            onNtfyTopicWasEditedChange(true)
          }
          onMethodChange(next)
        }}
        verification={verification}
        disabled={disabled}
      />
      {isSelfHostedMode && <TestDeliveryButton method={method} disabled={disabled} />}
    </div>
  )
}

export function TestDeliveryButton({ method, disabled = false }: { method: MethodDraft; disabled?: boolean }) {
  const t = useTranslations("walletNotifications")
  const [testing, setTesting] = useState(false)
  const [testSucceeded, setTestSucceeded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const successTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const requestVersion = useRef(0)
  const destinationKey = `${method.provider_type}:${method.notification_target.trim()}`
  const latestDestinationKey = useRef(destinationKey)
  latestDestinationKey.current = destinationKey
  const testable = ["ntfy", "nostr", "webhook"].includes(method.provider_type)
  useEffect(() => {
    requestVersion.current += 1
    if (successTimer.current) clearTimeout(successTimer.current)
    setTesting(false)
    setTestSucceeded(false)
    setError(null)
  }, [destinationKey])
  useEffect(() => () => {
    if (successTimer.current) clearTimeout(successTimer.current)
  }, [])
  if (!testable) return null

  const test = async () => {
    if (successTimer.current) clearTimeout(successTimer.current)
    const testedDestinationKey = destinationKey
    const requestId = ++requestVersion.current
    const providerType = method.provider_type
    const notificationTarget = method.notification_target.trim()
    setTesting(true)
    setTestSucceeded(false)
    setError(null)
    try {
      const response = providerType === "ntfy"
        ? await api.sendTestNtfyNotification(notificationTarget)
        : providerType === "nostr"
          ? await api.sendTestNostrNotification(notificationTarget)
          : await api.sendTestWebhookNotification(notificationTarget)
      if (requestId !== requestVersion.current || testedDestinationKey !== latestDestinationKey.current) return
      if (response.success) {
        setTestSucceeded(true)
        successTimer.current = setTimeout(() => setTestSucceeded(false), 3000)
      } else {
        setError(response.error || t("delivery.testFailed"))
      }
    } catch (caught) {
      if (requestId !== requestVersion.current || testedDestinationKey !== latestDestinationKey.current) return
      setError(caught instanceof Error ? caught.message : t("delivery.testFailed"))
    } finally {
      if (requestId === requestVersion.current && testedDestinationKey === latestDestinationKey.current) {
        setTesting(false)
      }
    }
  }

  const valid = Boolean(method.notification_target.trim()) &&
    (method.provider_type !== "webhook" || validateWebhookUrl(method.notification_target))

  return (
    <div className="space-y-2">
      <Button type="button" variant="outline" size="sm" onClick={test} disabled={disabled || testing || !valid}>
        {testing
          ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          : testSucceeded
            ? <Check className="h-4 w-4 text-green-600" aria-hidden="true" />
            : <Send className="h-4 w-4" aria-hidden="true" />}
        <span aria-live="polite">
          {testing ? t("delivery.testing") : testSucceeded ? t("delivery.testSent") : t("delivery.sendTest")}
        </span>
      </Button>
      {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
