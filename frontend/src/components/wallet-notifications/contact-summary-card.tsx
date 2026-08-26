"use client"

import { Loader2, MoreHorizontal, Pencil, Send, Trash2 } from "lucide-react"
import { useState } from "react"
import { useTranslations } from "next-intl"

import { DeleteContactModal } from "@/components/delete-contact-modal"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader } from "@/components/ui/card"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { api } from "@/lib/api"
import { satsToBtc } from "@/lib/utils"
import type { BalanceAlert, Contact } from "@/types"
import { PROVIDERS, ProviderIcon } from "./delivery-controls"
import type { MethodDraft } from "./types"
import { contactToDraft, getAlertSummary, getContentPreset, redactDeliveryTarget } from "./utils"

function balanceSummary(alerts: BalanceAlert[], t: ReturnType<typeof useTranslations<"walletNotifications">>) {
  if (alerts.length === 0) return t("summary.balance.none")
  if (alerts.length > 1) return t("summary.balance.count", { count: alerts.length })
  const alert = alerts[0]
  const amount = alert.threshold_currency && alert.threshold_fiat_amount
    ? `${alert.threshold_fiat_amount} ${alert.threshold_currency}`
    : `${satsToBtc(alert.threshold_sats)} BTC`
  return t("summary.balance.one", { condition: t(`alertTypes.${alert.alert_type}`), amount })
}

export function ContactSummaryCard({
  contact,
  alerts,
  isSelfHostedMode,
  isReadOnly,
  onEdit,
  onDeleted,
}: {
  contact: Contact
  alerts: BalanceAlert[]
  isSelfHostedMode: boolean
  isReadOnly: boolean
  onEdit: () => void
  onDeleted: () => void
}) {
  const t = useTranslations("walletNotifications")
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [testingProvider, setTestingProvider] = useState<string | null>(null)
  const [testError, setTestError] = useState<string | null>(null)
  const draft = contactToDraft(contact)
  const enabledMethods = draft.methods.filter((method) => method.is_enabled)
  const testableMethods = isSelfHostedMode
    ? enabledMethods.filter((method) => ["ntfy", "nostr", "webhook"].includes(method.provider_type))
    : []

  const sendTest = async (method: MethodDraft) => {
    setTestingProvider(method.provider_type)
    setTestError(null)
    try {
      const response = method.provider_type === "ntfy"
        ? await api.sendTestNtfyNotification(method.notification_target)
        : method.provider_type === "nostr"
          ? await api.sendTestNostrNotification(method.notification_target)
          : await api.sendTestWebhookNotification(method.notification_target)
      if (!response.success) setTestError(response.error || t("delivery.testFailed"))
    } catch (caught) {
      setTestError(caught instanceof Error ? caught.message : t("delivery.testFailed"))
    } finally {
      setTestingProvider(null)
    }
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="truncate text-base font-semibold">{contact.name}</h2>
              <Badge variant={contact.is_active && enabledMethods.length > 0 ? "secondary" : "outline"}>
                {contact.is_active && enabledMethods.length > 0 ? t("summary.active") : t("summary.inactive")}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">{t("summary.deliveryNotice")}</p>
          </div>
          {!isReadOnly && (
            <div className="flex items-center gap-1">
              <Button type="button" variant="outline" size="sm" onClick={onEdit}>
                <Pencil className="h-4 w-4" aria-hidden="true" />
                {t("contactActions.edit")}
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" variant="ghost" size="icon" aria-label={t("actions.contactMenu")}>
                    <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={onEdit}>
                    <Pencil className="mr-2 h-4 w-4" aria-hidden="true" />
                    {t("contactActions.edit")}
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem className="text-destructive focus:text-destructive" onClick={() => setDeleteOpen(true)}>
                    <Trash2 className="mr-2 h-4 w-4" aria-hidden="true" />
                    {t("contactActions.delete")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid gap-4 text-sm md:grid-cols-2">
          <SummaryItem label={t("summary.delivery")}>
            {draft.methods.length === 0 ? (
              <span>{t("delivery.noMethods")}</span>
            ) : (
              <span className="space-y-1">
                {draft.methods.map((method) => {
                  const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
                  return (
                    <span key={method.provider_type} className="flex items-center gap-2">
                      <ProviderIcon provider={provider} />
                      <span className="break-all">{provider.label}: {redactDeliveryTarget(method)}</span>
                      {!method.is_enabled && <Badge variant="outline">{t("summary.disabled")}</Badge>}
                    </span>
                  )
                })}
              </span>
            )}
          </SummaryItem>
          <SummaryItem label={t("summary.transactionAlerts")}>
            {t(`summary.alerts.${getAlertSummary(draft)}`)}
          </SummaryItem>
          <SummaryItem label={t("summary.messageContent")}>
            <span className="space-y-1">
              {draft.methods.map((method) => {
                const provider = PROVIDERS.find((item) => item.value === method.provider_type)
                return (
                  <span key={method.provider_type} className="block">
                    {provider?.label ?? method.provider_type}: {t(`privacy.presets.${getContentPreset(method.content_fields)}.label`)}
                  </span>
                )
              })}
            </span>
          </SummaryItem>
          <SummaryItem label={t("summary.balanceAlerts")}>
            {balanceSummary(alerts, t)}
          </SummaryItem>
        </dl>

        {testableMethods.length > 0 && !isReadOnly && (
          <div className="space-y-2 border-t pt-3">
            {testableMethods.length === 1 ? (
              <Button type="button" variant="outline" size="sm" onClick={() => sendTest(testableMethods[0])} disabled={Boolean(testingProvider)}>
                {testingProvider ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Send className="h-4 w-4" aria-hidden="true" />}
                {testingProvider ? t("delivery.testing") : t("delivery.sendTest")}
              </Button>
            ) : (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button type="button" variant="outline" size="sm" disabled={Boolean(testingProvider)}>
                    {testingProvider ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Send className="h-4 w-4" aria-hidden="true" />}
                    {testingProvider ? t("delivery.testing") : t("delivery.sendTest")}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent>
                  {testableMethods.map((method) => {
                    const provider = PROVIDERS.find((item) => item.value === method.provider_type) ?? PROVIDERS[0]
                    return (
                      <DropdownMenuItem key={method.provider_type} onClick={() => sendTest(method)}>
                        <ProviderIcon provider={provider} />
                        {provider.label}
                      </DropdownMenuItem>
                    )
                  })}
                </DropdownMenuContent>
              </DropdownMenu>
            )}
            {testError && <p role="alert" className="text-sm text-destructive">{testError}</p>}
          </div>
        )}
      </CardContent>

      <DeleteContactModal
        contact={contact}
        isOpen={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        onConfirmDelete={async () => {
          await api.deleteContact(contact.wallet_checksum, contact.id)
          onDeleted()
        }}
      />
    </Card>
  )
}

function SummaryItem({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd>{children}</dd>
    </div>
  )
}
