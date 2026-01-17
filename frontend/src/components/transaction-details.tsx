"use client"

import React from "react"
import { Mail, MessageCircle, Bell, XCircle, ArrowRight } from "lucide-react"
import { Transaction } from "../types"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"

interface ProviderIconProps {
  providerType: string
  className?: string
}

function ProviderIcon({ providerType, className }: ProviderIconProps) {
  switch (providerType) {
    case 'email':
      return <Mail className={className} />
    case 'sms':
    case 'twilio':
      return <MessageCircle className={className} />
    case 'ntfy':
    default:
      return <Bell className={className} />
  }
}

interface TransactionDetailsProps {
  transaction: Transaction
  isExpanded: boolean
}

export function TransactionDetails({ transaction, isExpanded }: TransactionDetailsProps) {
  const t = useTranslations('transactions')
  const { formatTransactionAmount, formatDateTime } = useFormatters()

  const renderNotificationGroup = (notifications: NonNullable<Transaction['notification_status']>) => {
    // Group notifications by contact name to avoid repetition
    const notificationsByContact = notifications.reduce((acc, notification) => {
      const contactName = notification.contact_name
      if (!acc[contactName]) acc[contactName] = []
      acc[contactName].push(notification)
      return acc
    }, {} as Record<string, typeof notifications>)

    return Object.entries(notificationsByContact)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([contactName, contactNotifications]) => {
        // Check if any notifications failed
        const hasErrors = contactNotifications.some(n =>
          n.status !== 'sent' && n.status !== 'delivered'
        )

        return (
          <div key={contactName} className="flex items-center gap-2 text-sm">
            <span className="font-medium">{contactName}:</span>
            <div className="flex items-center gap-1">
              {contactNotifications.map((notification, idx) => {
                const notificationTime = notification.created_at ? formatDateTime(notification.created_at) : 'Unknown time'
                const providerType = notification.provider_type || notification.provider_name.toLowerCase()
                const target = notification.notification_target || 'Unknown target'
                const hasError = notification.status !== 'sent' && notification.status !== 'delivered'

                let tooltipText = `${target}\n${t('notifications.sentAt')}: ${notificationTime}`
                if (hasError) {
                  tooltipText += `\n${t('notifications.statusLabel')}: ${notification.status}`
                  if (notification.error_message) {
                    tooltipText += `\n${t('notifications.errorLabel')}: ${notification.error_message}`
                  }
                }

                const iconClass = hasError ? "h-3 w-3 text-red-500" : "h-3 w-3"

                return (
                  <span
                    key={idx}
                    title={tooltipText}
                    className={hasError ? "cursor-help" : ""}
                  >
                    <ProviderIcon providerType={providerType} className={iconClass} />
                  </span>
                )
              })}
              {hasErrors && (
                <span title={t('tooltips.notificationsFailed')}>
                  <XCircle className="h-3 w-3 text-red-500 ml-1" />
                </span>
              )}
            </div>
          </div>
        )
      })
  }

  // Group notifications by type
  const groupedNotifications = transaction.notification_status?.reduce((acc, notification) => {
    const type = notification.notification_type === 'pending' ? 'pending' : 'confirmed'
    if (!acc[type]) acc[type] = []
    acc[type].push(notification)
    return acc
  }, {} as Record<string, NonNullable<Transaction['notification_status']>>) || {}

  const pendingNotifications = groupedNotifications.pending || []
  const confirmedNotifications = groupedNotifications.confirmed || []

  return (
    <div className={`px-4 transform transition-all duration-300 ease-out overflow-hidden ${isExpanded ? 'py-3 translate-y-0 max-h-96' : 'py-0 -translate-y-2 max-h-0'}`}>
      <div className="space-y-4">
        <div>
          <div className="space-y-1">
            <div className="flex items-center gap-3 text-sm">
              <span className="font-medium min-w-[80px]">{t('details.txid')}:</span>
              <a
                href={`https://mempool.space/tx/${transaction.txid}`}
                target="_blank"
                rel="noopener noreferrer"
                className="font-mono text-xs text-blue-600 hover:text-blue-800 underline"
                title={t('tooltips.viewOnMempool', { txid: transaction.txid })}
              >
                {transaction.txid.slice(0, 5)}...{transaction.txid.slice(-5)}
              </a>
            </div>
            {transaction.fee_sats && (
              <div className="flex items-center gap-3 text-sm">
                <span className="font-medium min-w-[80px]">{t('details.fee')}:</span>
                <span className="font-mono text-xs">{formatTransactionAmount(transaction.fee_sats)}</span>
              </div>
            )}
            {transaction.transaction_status === "replaced" && transaction.replaced_by_txid && (
              <div className="flex items-center gap-3 text-sm">
                <span className="font-medium min-w-[80px]">{t('details.replacedBy')}:</span>
                <a
                  href={`https://mempool.space/tx/${transaction.replaced_by_txid}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-mono text-xs text-orange-600 hover:text-orange-800 underline"
                  title={t('tooltips.viewOnMempool', { txid: transaction.replaced_by_txid })}
                >
                  {transaction.replaced_by_txid.slice(0, 5)}...{transaction.replaced_by_txid.slice(-5)}
                </a>
              </div>
            )}
            {transaction.replaced_at && (
              <div className="flex items-center gap-3 text-sm">
                <span className="font-medium min-w-[80px]">{t('details.replacedAt')}:</span>
                <span className="text-xs">{formatDateTime(transaction.replaced_at)}</span>
              </div>
            )}
          </div>
        </div>

        {/* Transaction Timeline - Show only when no contacts */}
        {(!transaction.notification_status || transaction.notification_status.length === 0) && (
          <div className="space-y-2">
            <div className="flex items-center gap-3 text-xs text-muted-foreground uppercase">
              <span className="font-semibold">
                {t('timeline.pending')} - {formatDateTime(transaction.first_seen_at)}
              </span>
              {transaction.confirmed_at && (
                <>
                  <ArrowRight className="h-3 w-3" />
                  <span className="font-semibold">
                    {t('timeline.confirmed')} - {formatDateTime(transaction.confirmed_at)}
                  </span>
                </>
              )}
            </div>
          </div>
        )}

        {/* Notifications */}
        {transaction.notification_status && transaction.notification_status.length > 0 && (
          <div className="space-y-3">
            {/* Content Row */}
            <div className="flex justify-between items-center gap-4">
              <div className="flex-1">
                {pendingNotifications.length > 0 && (
                  <div className="border rounded-md bg-muted/30 p-3">
                    <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
                      {t('timeline.pending')} - {formatDateTime(transaction.first_seen_at)}
                    </h5>
                    <div className="space-y-2">
                      {renderNotificationGroup(pendingNotifications)}
                    </div>
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
                  <div className="border rounded-md bg-muted/30 p-3">
                    <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
                      {t('timeline.confirmed')}{transaction.confirmed_at ? ` - ${formatDateTime(transaction.confirmed_at)}` : ''}
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
