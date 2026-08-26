"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"

import { AlertTimingControls } from "./alert-controls"
import { BalanceDraftControls } from "./balance-draft-controls"
import { ContentPresetControls } from "./content-presets"
import {
  DeliveryStepFields,
  isMethodVerified,
  useDeliveryVerification,
} from "./delivery-controls"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader } from "@/components/ui/card"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError } from "@/lib/utils"
import { validateWebhookUrl } from "@/components/contact-modal/index"
import { DEFAULT_NOTIFICATION_CONTENT_FIELDS } from "@/components/notification-content-fields-control"
import type { BalanceDraft, ContactDraft, MethodDraft, WizardStep } from "./types"
import { DEFAULT_NEW_CONTACT_SETTINGS, generatePrivateNtfyTopic, txSettingsFromDraft } from "./utils"

const STEPS: WizardStep[] = ["delivery", "alerts", "privacy"]

export function ContactCreationWizard({
  walletChecksum,
  isSelfHostedMode,
  registeredProviderNames,
  preferredFiatCurrency,
  onCancel,
  onCreated,
}: {
  walletChecksum: string
  isSelfHostedMode: boolean
  registeredProviderNames: string[]
  preferredFiatCurrency: string
  onCancel: () => void
  onCreated: (failedBalanceAlerts?: BalanceDraft[]) => void
}) {
  const t = useTranslations("walletNotifications")
  const tContacts = useTranslations("contacts")
  const tApiErrors = useTranslations("errors.api")
  const initialMethod = useRef<MethodDraft>({
    provider_type: isSelfHostedMode ? "ntfy" : "email",
    notification_target: isSelfHostedMode ? generatePrivateNtfyTopic() : "",
    is_enabled: true,
    content_fields: { ...DEFAULT_NOTIFICATION_CONTENT_FIELDS },
  })
  const [step, setStep] = useState<WizardStep>("delivery")
  const [draft, setDraft] = useState<ContactDraft>({
    name: "",
    methods: [{ ...initialMethod.current }],
    ...DEFAULT_NEW_CONTACT_SETTINGS,
  })
  const [balanceDrafts, setBalanceDrafts] = useState<BalanceDraft[]>([])
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [hasUserChanges, setHasUserChanges] = useState(false)
  const [ntfyTopicWasEdited, setNtfyTopicWasEdited] = useState(false)
  const headingRef = useRef<HTMLHeadingElement>(null)
  const verification = useDeliveryVerification({
    walletChecksum,
    contactName: draft.name,
    originalSmsTarget: null,
    originalEmailTarget: null,
    onError: setError,
  })
  const method = draft.methods[0]
  const stepIndex = STEPS.indexOf(step)
  const isDirty = hasUserChanges || step !== "delivery" || balanceDrafts.length > 0

  useEffect(() => {
    headingRef.current?.focus()
  }, [step])

  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (!isDirty) return
      event.preventDefault()
      event.returnValue = true
    }
    window.addEventListener("beforeunload", warn)
    return () => window.removeEventListener("beforeunload", warn)
  }, [isDirty])

  const cancel = () => {
    if (isDirty && !window.confirm(t("discard.confirm"))) return
    onCancel()
  }

  const validateDelivery = () => {
    if (!draft.name.trim()) return tContacts("errors.nameRequired")
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
    if (!isMethodVerified(method, verification)) {
      return method.provider_type === "sms"
        ? tContacts("verification.verifyNewSms")
        : tContacts("verification.verifyNewEmail")
    }
    return null
  }

  const continueToNext = () => {
    if (step === "delivery") {
      const validationError = validateDelivery()
      if (validationError) {
        setError(validationError)
        return
      }
    }
    setError(null)
    setStep(STEPS[stepIndex + 1])
  }

  const create = async () => {
    const validationError = validateDelivery()
    if (validationError) {
      setStep("delivery")
      setError(validationError)
      return
    }
    setCreating(true)
    setError(null)
    try {
      const created = await api.createContact(
        walletChecksum,
        draft.name.trim(),
        [{
          provider_type: method.provider_type,
          notification_target:
            method.provider_type === "email"
              ? verification.email.verificationAddress || method.notification_target.trim()
              : method.provider_type === "sms"
                ? verification.sms.verificationPhone || method.notification_target.trim()
                : method.notification_target.trim(),
          is_enabled: true,
          content_fields: method.content_fields,
        }],
        txSettingsFromDraft(draft)
      )

      const results = await Promise.allSettled(
        balanceDrafts.map((alert) => api.createBalanceAlert(walletChecksum, {
          contact_id: created.id,
          alert_type: alert.alert_type,
          threshold_sats: alert.threshold_sats,
          threshold_currency: alert.threshold_currency,
          threshold_fiat_amount: alert.threshold_fiat_amount,
        }))
      )
      const failed = balanceDrafts.filter((_, index) => results[index].status === "rejected")
      onCreated(failed.length > 0 ? failed : undefined)
    } catch (caught) {
      setError(
        caught instanceof ApiError
          ? getTranslatedApiError(caught, tApiErrors)
          : t("errors.createFailed")
      )
    } finally {
      setCreating(false)
    }
  }

  return (
    <Card>
      <CardHeader className="space-y-4">
        <div className="flex items-center justify-between gap-4">
          <h2 className="text-base font-semibold">{t("wizard.title")}</h2>
          <Button type="button" variant="ghost" size="sm" onClick={cancel} disabled={creating}>
            {t("actions.cancel")}
          </Button>
        </div>
        <ol className="grid grid-cols-3 gap-2" aria-label={t("wizard.progressLabel")}>
          {STEPS.map((item, index) => (
            <li
              key={item}
              aria-current={item === step ? "step" : undefined}
              className={`rounded-md border px-3 py-2 text-xs ${item === step ? "border-primary font-medium" : "text-muted-foreground"}`}
            >
              <span className="block">{t("wizard.step", { current: index + 1, total: STEPS.length })}</span>
              {t(`wizard.steps.${item}.short`)}
            </li>
          ))}
        </ol>
        <p className="sr-only" aria-live="polite">
          {t("wizard.currentStep", { current: stepIndex + 1, total: STEPS.length, name: t(`wizard.steps.${step}.title`) })}
        </p>
      </CardHeader>
      <CardContent className="space-y-6">
        <section className="space-y-4">
          <div>
            <h3 ref={headingRef} tabIndex={-1} className="text-lg font-semibold outline-none">
              {t(`wizard.steps.${step}.title`)}
            </h3>
            <p className="mt-1 text-sm text-muted-foreground">{t(`wizard.steps.${step}.description`)}</p>
          </div>

          {step === "delivery" && (
            <DeliveryStepFields
              name={draft.name}
              onNameChange={(name) => { setHasUserChanges(true); setDraft((current) => ({ ...current, name })); setError(null) }}
              method={method}
              onMethodChange={(next) => { setDraft((current) => ({ ...current, methods: [next] })); setError(null) }}
              isSelfHostedMode={isSelfHostedMode}
              registeredProviderNames={registeredProviderNames}
              verification={verification}
              ntfyTopicWasEdited={ntfyTopicWasEdited}
              onNtfyTopicWasEditedChange={setNtfyTopicWasEdited}
              onDirty={() => setHasUserChanges(true)}
              disabled={creating}
            />
          )}
          {step === "alerts" && (
            <div className="space-y-4">
              <AlertTimingControls draft={draft} onChange={setDraft} disabled={creating} />
              <BalanceDraftControls
                walletChecksum={walletChecksum}
                value={balanceDrafts}
                onChange={setBalanceDrafts}
                preferredFiatCurrency={preferredFiatCurrency}
                disabled={creating}
              />
            </div>
          )}
          {step === "privacy" && (
            <ContentPresetControls
              value={method.content_fields}
              onChange={(content_fields) => setDraft((current) => ({
                ...current,
                methods: [{ ...current.methods[0], content_fields }],
              }))}
              hasBalanceAlerts={balanceDrafts.length > 0}
              disabled={creating}
            />
          )}
        </section>

        {error && <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>}

        <div className="flex justify-between border-t pt-4">
          <Button
            type="button"
            variant="ghost"
            onClick={() => setStep(STEPS[stepIndex - 1])}
            disabled={stepIndex === 0 || creating}
          >
            {t("actions.back")}
          </Button>
          {stepIndex < STEPS.length - 1 ? (
            <Button type="button" onClick={continueToNext} disabled={creating}>
              {t("actions.continue")}
            </Button>
          ) : (
            <Button type="button" onClick={create} disabled={creating}>
              {creating ? t("actions.creating") : t("actions.create")}
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
