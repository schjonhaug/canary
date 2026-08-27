"use client"

import { Plus, Save, Trash2 } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"

import { validateWebhookUrl } from "@/components/contact-modal/index"
import { DEFAULT_NOTIFICATION_CONTENT_FIELDS } from "@/components/notification-content-fields-control"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useNtfyServerTarget } from "@/hooks/useNtfyServerUrl"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError } from "@/lib/utils"
import type { BalanceAlert, Contact } from "@/types"
import { AlertTimingControls } from "./alert-controls"
import { BalanceDraftControls, formatBalanceDraft } from "./balance-draft-controls"
import { ContentPresetControls } from "./content-presets"
import {
  availableProviders,
  DeliveryTargetFields,
  isMethodVerified,
  PROVIDERS,
  ProviderIcon,
  TestDeliveryButton,
  useDeliveryVerification,
} from "./delivery-controls"
import type { BalanceDraft, ContactDraft, MethodDraft, NotificationProvider } from "./types"
import {
  alertsToDrafts,
  contactToDraft,
  generatePrivateNtfyTopic,
  isDraftDirty,
  txSettingsFromDraft,
} from "./utils"

export function ContactEditor({
  contact,
  alerts,
  walletChecksum,
  isSelfHostedMode,
  registeredProviderNames,
  preferredFiatCurrency,
  onCancel,
  onSaved,
}: {
  contact: Contact
  alerts: BalanceAlert[]
  walletChecksum: string
  isSelfHostedMode: boolean
  registeredProviderNames: string[]
  preferredFiatCurrency: string
  onCancel: () => void
  onSaved: (failedOperations?: string[]) => void
}) {
  const t = useTranslations("walletNotifications")
  const tContacts = useTranslations("contacts")
  const tApiErrors = useTranslations("errors.api")
  const initialDraft = useMemo(() => contactToDraft(contact), [contact])
  const initialBalanceDrafts = useMemo(() => alertsToDrafts(alerts), [alerts])
  const [draft, setDraft] = useState<ContactDraft>(initialDraft)
  const [balanceDrafts, setBalanceDrafts] = useState<BalanceDraft[]>(initialBalanceDrafts)
  const [providerToAdd, setProviderToAdd] = useState<NotificationProvider>(isSelfHostedMode ? "ntfy" : "email")
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const ntfyServerTarget = useNtfyServerTarget()
  const providers = useMemo(
    () => availableProviders(isSelfHostedMode, registeredProviderNames),
    [isSelfHostedMode, registeredProviderNames]
  )
  const addableProviders = providers.filter((provider) =>
    !draft.methods.some((method) => method.provider_type === provider.value)
  )
  const selectedProvider = addableProviders.find((provider) => provider.value === providerToAdd) ?? addableProviders[0]
  const verification = useDeliveryVerification({
    walletChecksum,
    contactName: draft.name,
    originalSmsTarget: initialDraft.methods.find((method) => method.provider_type === "sms")?.notification_target ?? null,
    originalEmailTarget: initialDraft.methods.find((method) => method.provider_type === "email")?.notification_target ?? null,
    onError: setError,
  })
  const dirty = isDraftDirty(draft, initialDraft, balanceDrafts, initialBalanceDrafts)

  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (!dirty) return
      event.preventDefault()
      event.returnValue = true
    }
    window.addEventListener("beforeunload", warn)
    return () => window.removeEventListener("beforeunload", warn)
  }, [dirty])

  const cancel = () => {
    if (dirty && !window.confirm(t("discard.confirm"))) return
    setDraft(initialDraft)
    setBalanceDrafts(initialBalanceDrafts)
    verification.sms.reset()
    verification.email.reset()
    onCancel()
  }

  const validationError = () => {
    if (!draft.name.trim()) return tContacts("errors.nameRequired")
    if (draft.methods.length === 0) return t("errors.deliveryRequired")
    for (const method of draft.methods) {
      if (!method.notification_target.trim()) {
        if (method.provider_type === "ntfy") return tContacts("errors.ntfyTopicRequired")
        if (method.provider_type === "nostr") return tContacts("errors.nostrRecipientRequired")
        if (method.provider_type === "webhook") return tContacts("errors.webhookUrlRequired")
        if (method.provider_type === "sms") return tContacts("errors.phoneRequired")
        return tContacts("errors.emailRequired")
      }
      if (method.provider_type === "webhook" && !validateWebhookUrl(method.notification_target)) {
        return tContacts("add.webhook.invalidUrl")
      }
      const original = initialDraft.methods.find((item) => item.provider_type === method.provider_type)
      if (!isMethodVerified(method, verification, original)) {
        return method.provider_type === "sms"
          ? tContacts("verification.verifyNewSms")
          : tContacts("verification.verifyNewEmail")
      }
    }
    return null
  }

  const save = async () => {
    const invalid = validationError()
    if (invalid) {
      setError(invalid)
      return
    }
    setSaving(true)
    setError(null)
    try {
      await api.updateContact(
        walletChecksum,
        contact.id,
        draft.name.trim(),
        draft.methods.map((method) => ({
          provider_type: method.provider_type,
          notification_target:
            method.provider_type === "email" && !initialDraft.methods.some((item) => item.provider_type === "email" && item.notification_target === method.notification_target)
              ? verification.email.verificationAddress || method.notification_target.trim()
              : method.provider_type === "sms" && !initialDraft.methods.some((item) => item.provider_type === "sms" && item.notification_target === method.notification_target)
                ? verification.sms.verificationPhone || method.notification_target.trim()
                : method.notification_target.trim(),
          is_enabled: method.is_enabled,
          content_fields: method.content_fields,
        })),
        txSettingsFromDraft(draft)
      )

      const deletedAlerts = initialBalanceDrafts.filter((initial) =>
        !balanceDrafts.some((current) => current.id === initial.id)
      )
      const newAlerts = balanceDrafts.filter((alert) => !alert.persisted)
      const deleteOperations = deletedAlerts.map((alert) => ({
        description: t("partial.deleteOperation", {
          condition: t(`alertTypes.${alert.alert_type}`),
          amount: formatBalanceDraft(alert),
        }),
        run: () => api.deleteBalanceAlert(alert.id),
      }))
      const deleteResults = await Promise.allSettled(
        deleteOperations.map((operation) => operation.run())
      )

      const createOperations = newAlerts.map((alert) => ({
          description: t("partial.createOperation", {
            condition: t(`alertTypes.${alert.alert_type}`),
            amount: formatBalanceDraft(alert),
          }),
          run: () => api.createBalanceAlert(walletChecksum, {
            contact_id: contact.id,
            alert_type: alert.alert_type,
            threshold_sats: alert.threshold_sats,
            threshold_currency: alert.threshold_currency,
            threshold_fiat_amount: alert.threshold_fiat_amount,
          }),
        }))
      const createResults = await Promise.allSettled(
        createOperations.map((operation) => operation.run())
      )
      const failedOperations = [
        ...deleteOperations
          .filter((_, index) => deleteResults[index].status === "rejected")
          .map((operation) => operation.description),
        ...createOperations
          .filter((_, index) => createResults[index].status === "rejected")
          .map((operation) => operation.description),
      ]
      onSaved(failedOperations.length > 0 ? failedOperations : undefined)
    } catch (caught) {
      setError(
        caught instanceof ApiError
          ? getTranslatedApiError(caught, tApiErrors)
          : t("errors.saveFailed")
      )
    } finally {
      setSaving(false)
    }
  }

  const updateMethod = (index: number, method: MethodDraft) => {
    setDraft((current) => ({
      ...current,
      methods: current.methods.map((item, itemIndex) => itemIndex === index ? method : item),
    }))
    setError(null)
  }

  const addMethod = () => {
    if (!selectedProvider) return
    const notification_target = selectedProvider.value === "ntfy"
      ? ntfyServerTarget.defaultTopic || generatePrivateNtfyTopic()
      : ""
    setDraft((current) => ({
      ...current,
      methods: [...current.methods, {
        provider_type: selectedProvider.value,
        notification_target,
        is_enabled: true,
        content_fields: { ...DEFAULT_NOTIFICATION_CONTENT_FIELDS },
      }],
    }))
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold">{t("editor.title", { name: contact.name })}</h2>
            <p className="mt-1 text-sm text-muted-foreground">{t("editor.saveHint")}</p>
          </div>
          <Button type="button" variant="ghost" size="sm" onClick={cancel} disabled={saving}>
            {t("actions.cancel")}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-8">
        <EditorSection title={t("wizard.steps.delivery.title")} description={t("wizard.steps.delivery.description")}>
          <div className="space-y-2">
            <label htmlFor={`contact-name-${contact.id}`} className="text-sm font-medium">{t("delivery.name")}</label>
            <Input
              id={`contact-name-${contact.id}`}
              value={draft.name}
              onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
              disabled={saving}
            />
          </div>
          <div className="space-y-4">
            {draft.methods.map((method, index) => {
              const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
              const original = initialDraft.methods.find((item) => item.provider_type === method.provider_type)
              return (
                <div key={`${method.provider_type}-${index}`} className="space-y-3 rounded-md border p-3">
                  <div className="flex items-center justify-between gap-3">
                    <label className="flex items-center gap-2 text-sm font-medium">
                      <Checkbox
                        checked={method.is_enabled}
                        disabled={saving || (draft.methods.length === 1 && method.is_enabled)}
                        onCheckedChange={(checked) => updateMethod(index, { ...method, is_enabled: checked === true })}
                      />
                      <ProviderIcon provider={provider} />
                      {provider.label}
                    </label>
                    {draft.methods.length > 1 && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        onClick={() => setDraft((current) => ({ ...current, methods: current.methods.filter((_, itemIndex) => itemIndex !== index) }))}
                        aria-label={t("actions.removeDelivery")}
                        disabled={saving}
                      >
                        <Trash2 className="h-4 w-4" aria-hidden="true" />
                      </Button>
                    )}
                  </div>
                  <DeliveryTargetFields
                    method={method}
                    onChange={(next) => updateMethod(index, next)}
                    verification={verification}
                    originalMethod={original}
                    disabled={saving}
                  />
                  {isSelfHostedMode && <TestDeliveryButton method={method} disabled={saving} />}
                </div>
              )
            })}
          </div>
          {selectedProvider && (
            <div className="flex flex-wrap items-center gap-2">
              {addableProviders.length > 1 && (
                <Select value={selectedProvider.value} onValueChange={(value) => setProviderToAdd(value as NotificationProvider)}>
                  <SelectTrigger className="w-40" aria-label={t("delivery.method")}><SelectValue /></SelectTrigger>
                  <SelectContent>
                    {addableProviders.map((provider) => <SelectItem key={provider.value} value={provider.value}>{provider.label}</SelectItem>)}
                  </SelectContent>
                </Select>
              )}
              <Button type="button" variant="outline" size="sm" onClick={addMethod} disabled={saving}>
                <Plus className="h-4 w-4" aria-hidden="true" />
                {t("editor.addDeliveryMethod")}
              </Button>
            </div>
          )}
        </EditorSection>

        <EditorSection title={t("wizard.steps.alerts.title")} description={t("wizard.steps.alerts.description")}>
          <AlertTimingControls draft={draft} onChange={setDraft} disabled={saving} />
          <BalanceDraftControls
            walletChecksum={walletChecksum}
            value={balanceDrafts}
            onChange={setBalanceDrafts}
            preferredFiatCurrency={preferredFiatCurrency}
            disabled={saving}
          />
        </EditorSection>

        <EditorSection title={t("wizard.steps.privacy.title")} description={t("editor.privacyHint")}>
          {draft.methods.map((method, index) => {
            const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
            return (
              <div key={`${method.provider_type}-${index}`} className="space-y-3 rounded-md border p-3">
                <h4 className="flex items-center gap-2 text-sm font-medium">
                  <ProviderIcon provider={provider} />
                  {provider.label}
                </h4>
                <ContentPresetControls
                  value={method.content_fields}
                  onChange={(content_fields) => updateMethod(index, { ...method, content_fields })}
                  hasBalanceAlerts={balanceDrafts.length > 0}
                  disabled={saving}
                />
              </div>
            )
          })}
        </EditorSection>

        {error && <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>}

        <div className="flex justify-end gap-2 border-t pt-4">
          <Button type="button" variant="ghost" onClick={cancel} disabled={saving}>{t("actions.cancel")}</Button>
          <Button type="button" onClick={save} disabled={saving || !dirty}>
            <Save className="h-4 w-4" aria-hidden="true" />
            {saving ? t("actions.saving") : t("actions.save")}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}

function EditorSection({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: React.ReactNode
}) {
  return (
    <section className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{description}</p>
      </div>
      {children}
    </section>
  )
}
