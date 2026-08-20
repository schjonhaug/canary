"use client"

import React from "react"
import { ArrowRight, Bell, Loader2, Mail, MessageCircle, RadioTower, XCircle } from "lucide-react"
import { NotificationStatus, Transaction } from "../types"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import { useTxExplorer } from "@/hooks/useTxExplorer"
import { buildTransactionExplorerUrl } from "@/lib/tx-explorers"

interface ProviderIconProps {
  providerType: string
  className?: string
}

function ProviderIcon({ providerType, className }: ProviderIconProps) {
  switch (providerType) {
    case "email":
      return <Mail className={className} />
    case "sms":
    case "twilio":
      return <MessageCircle className={className} />
    case "nostr":
      return <RadioTower className={className} />
    case "ntfy":
    default:
      return <Bell className={className} />
  }
}

interface TransactionDetailsProps {
  transaction: Transaction
  isExpanded: boolean
  notifications?: NotificationStatus[]
  isLoadingNotifications?: boolean
  notificationError?: string | null
}

export function TransactionDetails({
  transaction,
  isExpanded,
  notifications,
  isLoadingNotifications = false,
  notificationError = null,
}: TransactionDetailsProps) {
  const t = useTranslations("transactions")
  const { formatTransactionAmount, formatDateTime } = useFormatters()
  const resolvedNotifications = notifications ?? transaction.notification_status ?? []
  const txExplorer = useTxExplorer()

  const renderNotificationGroup = (notificationsToRender: NotificationStatus[]) => {
    const notificationsByContact = notificationsToRender.reduce((acc, notification) => {
      const contactName = notification.contact_name
      if (!acc[contactName]) acc[contactName] = []
      acc[contactName].push(notification)
      return acc
    }, {} as Record<string, NotificationStatus[]>)

    return Object.entries(notificationsByContact)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([contactName, contactNotifications]) => {
        const hasErrors = contactNotifications.some(
          (notification) => notification.status !== "sent" && notification.status !== "delivered"
        )

        return (
          <div key={contactName} className="flex items-center gap-2 text-sm">
            <span className="font-medium">{contactName}:</span>
            <div className="flex items-center gap-1">
              {contactNotifications.map((notification, index) => {
                const notificationTime = notification.created_at
                  ? formatDateTime(notification.created_at)
                  : t("notifications.unknownTime")
                const providerType =
                  notification.provider_type || notification.provider_name.toLowerCase()
                const target =
                  notification.notification_target || t("notifications.unknownTarget")
                const hasError =
                  notification.status !== "sent" && notification.status !== "delivered"

                let tooltipText = `${target}\n${t("notifications.sentAt")}: ${notificationTime}`
                if (hasError) {
                  tooltipText += `\n${t("notifications.statusLabel")}: ${notification.status}`
                  if (notification.error_message) {
                    tooltipText += `\n${t("notifications.errorLabel")}: ${notification.error_message}`
                  }
                }

                return (
                  <span
                    key={index}
                    title={tooltipText}
                    className={hasError ? "cursor-help" : ""}
                  >
                    <ProviderIcon
                      providerType={providerType}
                      className={hasError ? "h-3 w-3 text-red-500" : "h-3 w-3"}
                    />
                  </span>
                )
              })}
              {hasErrors && (
                <span title={t("tooltips.notificationsFailed")}>
                  <XCircle className="ml-1 h-3 w-3 text-red-500" />
                </span>
              )}
            </div>
          </div>
        )
      })
  }

  const groupedNotifications = resolvedNotifications.reduce((acc, notification) => {
    const type = notification.notification_type === "pending" ? "pending" : "confirmed"
    if (!acc[type]) acc[type] = []
    acc[type].push(notification)
    return acc
  }, {} as Record<string, NotificationStatus[]>)

  const pendingNotifications = groupedNotifications.pending || []
  const confirmedNotifications = groupedNotifications.confirmed || []

  return (
    <div
      className={`overflow-hidden px-4 transition-all duration-300 ease-out ${
        isExpanded ? "max-h-96 translate-y-0 py-3" : "max-h-0 -translate-y-2 py-0"
      }`}
    >
      <div className="space-y-4">
        <div className="space-y-1">
          <div className="flex items-center gap-3 text-sm">
            <span className="min-w-[80px] font-medium">{t("details.txid")}:</span>
            <a
              href={buildTransactionExplorerUrl(txExplorer.baseUrl, transaction.txid)}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-xs text-blue-600 underline hover:text-blue-800"
              title={t("tooltips.viewOnExplorer", {
                txid: transaction.txid,
                explorer: txExplorer.name,
              })}
            >
              {transaction.txid.slice(0, 5)}...{transaction.txid.slice(-5)}
            </a>
          </div>
          {transaction.fee_sats && (
            <div className="flex items-center gap-3 text-sm">
              <span className="min-w-[80px] font-medium">{t("details.fee")}:</span>
              <span className="font-mono text-xs">
                {formatTransactionAmount(transaction.fee_sats)}
              </span>
            </div>
          )}
          {transaction.transaction_status === "replaced" && transaction.replaced_by_txid && (
            <div className="flex items-center gap-3 text-sm">
              <span className="min-w-[80px] font-medium">{t("details.replacedBy")}:</span>
              <a
                href={buildTransactionExplorerUrl(txExplorer.baseUrl, transaction.replaced_by_txid)}
                target="_blank"
                rel="noopener noreferrer"
                className="font-mono text-xs text-orange-600 underline hover:text-orange-800"
                title={t("tooltips.viewOnExplorer", {
                  txid: transaction.replaced_by_txid,
                  explorer: txExplorer.name,
                })}
              >
                {transaction.replaced_by_txid.slice(0, 5)}...
                {transaction.replaced_by_txid.slice(-5)}
              </a>
            </div>
          )}
          {transaction.replaced_at && (
            <div className="flex items-center gap-3 text-sm">
              <span className="min-w-[80px] font-medium">{t("details.replacedAt")}:</span>
              <span className="text-xs">{formatDateTime(transaction.replaced_at)}</span>
            </div>
          )}
        </div>

        {isLoadingNotifications && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            <span>{t("loading")}</span>
          </div>
        )}

        {notificationError && !isLoadingNotifications && (
          <p className="text-sm text-destructive">{notificationError}</p>
        )}

        {!isLoadingNotifications && resolvedNotifications.length === 0 && (
          <div className="space-y-2">
            <div className="flex items-center gap-3 text-xs uppercase text-muted-foreground">
              <span className="font-semibold">
                {t("timeline.pending")} - {formatDateTime(transaction.first_seen_at)}
              </span>
              {transaction.confirmed_at && (
                <>
                  <ArrowRight className="h-3 w-3" />
                  <span className="font-semibold">
                    {t("timeline.confirmed")} - {formatDateTime(transaction.confirmed_at)}
                  </span>
                </>
              )}
            </div>
          </div>
        )}

        {!isLoadingNotifications && resolvedNotifications.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-4">
              <div className="flex-1">
                {pendingNotifications.length > 0 && (
                  <div className="rounded-md border bg-muted/30 p-3">
                    <h5 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                      {t("timeline.pending")} - {formatDateTime(transaction.first_seen_at)}
                    </h5>
                    <div className="space-y-2">{renderNotificationGroup(pendingNotifications)}</div>
                  </div>
                )}
              </div>
              {pendingNotifications.length > 0 && confirmedNotifications.length > 0 && (
                <div className="flex items-center justify-center px-2">
                  <ArrowRight className="h-4 w-4 text-muted-foreground" />
                </div>
              )}
              <div className="flex-1">
                {confirmedNotifications.length > 0 && (
                  <div className="rounded-md border bg-muted/30 p-3">
                    <h5 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                      {t("timeline.confirmed")}
                      {transaction.confirmed_at
                        ? ` - ${formatDateTime(transaction.confirmed_at)}`
                        : ""}
                    </h5>
                    <div className="space-y-2">
                      {renderNotificationGroup(confirmedNotifications)}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
