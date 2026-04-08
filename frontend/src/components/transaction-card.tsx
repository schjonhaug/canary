"use client"

import React, { useState } from "react"
import { Card, CardContent } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import {
  CheckCircle,
  Baby,
  Mail,
  MessageCircle,
  Bell,
  ChevronDown,
  ChevronRight,
  XCircle,
  Loader2,
  ArrowRight,
  Clock
} from "lucide-react"
import { Transaction } from "../types"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import { useMempoolUrl } from "@/hooks/useMempoolUrl"

interface TransactionCardProps {
  transaction: Transaction
  showWalletName: boolean
}

export function TransactionCard({ transaction, showWalletName }: TransactionCardProps) {
  const [isExpanded, setIsExpanded] = useState(false)
  const t = useTranslations('transactions')
  const { formatTransactionAmount, formatDateTime } = useFormatters()
  const mempoolBaseUrl = useMempoolUrl()

  const renderNotificationDetails = () => {
    if (!transaction.notification_status || transaction.notification_status.length === 0) return null

    const groupedNotifications = transaction.notification_status.reduce((acc, notification) => {
      const type = notification.notification_type === 'pending' ? 'pending' : 'confirmed'
      if (!acc[type]) acc[type] = []
      acc[type].push(notification)
      return acc
    }, {} as Record<string, typeof transaction.notification_status>)

    const pendingNotifications = groupedNotifications.pending || []
    const confirmedNotifications = groupedNotifications.confirmed || []

    // const getPendingTimestamp = () => {
    //   if (pendingNotifications.length === 0) return null
    //   try {
    //     const earliest = pendingNotifications.reduce((earliest, current) => {
    //       if (!earliest?.created_at || !current?.created_at) return earliest || current
    //       const earliestTime = new Date(earliest.created_at).getTime()
    //       const currentTime = new Date(current.created_at).getTime()
    //       if (isNaN(earliestTime) || isNaN(currentTime)) return earliest
    //       return currentTime < earliestTime ? current : earliest
    //     })
    //     if (!earliest?.created_at) return null
    //     const formatted = formatDateTime(earliest.created_at)
    //     return formatted === "Invalid date" ? null : formatted
    //   } catch (e) {
    //     console.error("Error getting pending timestamp:", e)
    //     return null
    //   }
    // }

    // const getConfirmedTimestamp = () => {
    //   if (confirmedNotifications.length === 0) return null
    //   try {
    //     const earliest = confirmedNotifications.reduce((earliest, current) => {
    //       if (!earliest?.created_at || !current?.created_at) return earliest || current
    //       const earliestTime = new Date(earliest.created_at).getTime()
    //       const currentTime = new Date(current.created_at).getTime()
    //       if (isNaN(earliestTime) || isNaN(currentTime)) return earliest
    //       return currentTime < earliestTime ? current : earliest
    //     })
    //     if (!earliest?.created_at) return null
    //     const formatted = formatDateTime(earliest.created_at)
    //     return formatted === "Invalid date" ? null : formatted
    //   } catch (e) {
    //     console.error("Error getting confirmed timestamp:", e)
    //     return null
    //   }
    // }

    const renderNotificationGroup = (notifications: typeof transaction.notification_status, title: string) => {
      const notificationsByContact = notifications.reduce((acc, notification) => {
        const contactName = notification.contact_name
        if (!acc[contactName]) acc[contactName] = []
        acc[contactName].push(notification)
        return acc
      }, {} as Record<string, typeof notifications>)

      return (
        <div className="space-y-2">
          {title && <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">{title}</h5>}
          <div className="space-y-1">
            {Object.entries(notificationsByContact)
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([contactName, contactNotifications]) => {
                // Check if any notifications failed
                const hasErrors = contactNotifications.some(n =>
                  n.status !== 'sent' && n.status !== 'delivered'
                )

                return (
                  <div key={contactName} className="flex items-center gap-2 text-sm">
                    <span className="font-medium flex-shrink-0">{contactName}:</span>
                    <div className="flex items-center gap-1 flex-wrap">
                      {contactNotifications.map((notification, idx) => {
                        const notificationTime = notification.created_at ? formatDateTime(notification.created_at) : t('notifications.unknownTime')
                        const providerType = notification.provider_type || notification.provider_name.toLowerCase()
                        const target = notification.notification_target || t('notifications.unknownTarget')
                        const hasError = notification.status !== 'sent' && notification.status !== 'delivered'

                        let tooltipText = `${target}\n${t('notifications.sentAt')}: ${notificationTime}`
                        if (hasError) {
                          tooltipText += `\n${t('notifications.statusLabel')}: ${notification.status}`
                          if (notification.error_message) {
                            tooltipText += `\n${t('notifications.errorLabel')}: ${notification.error_message}`
                          }
                        }

                        const iconClass = hasError ? "h-3 w-3 text-red-500 flex-shrink-0" : "h-3 w-3 flex-shrink-0"

                        return (
                          <span
                            key={idx}
                            title={tooltipText}
                            className={hasError ? "cursor-help" : ""}
                          >
                            {(() => {
                              switch (providerType) {
                                case 'email':
                                  return <Mail className={iconClass} />
                                case 'sms':
                                case 'twilio':
                                  return <MessageCircle className={iconClass} />
                                case 'ntfy':
                                default:
                                  return <Bell className={iconClass} />
                              }
                            })()}
                          </span>
                        )
                      })}
                      {hasErrors && (
                        <span title={t('tooltips.notificationsFailed')}>
                          <XCircle className="h-3 w-3 text-red-500 ml-1 flex-shrink-0" />
                        </span>
                      )}
                    </div>
                  </div>
                )
              })}
          </div>
        </div>
      )
    }

    return (
      <div className="space-y-3 mt-3 pt-3 border-t">
        {pendingNotifications.length > 0 && (
          <div className="border rounded-md bg-muted/30 p-3 space-y-2">
            <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              {t('timeline.pending')} - {formatDateTime(transaction.first_seen_at)}
            </h5>
            {renderNotificationGroup(pendingNotifications, "")}
          </div>
        )}
        {pendingNotifications.length > 0 && confirmedNotifications.length > 0 && (
          <div className="flex justify-center py-2">
            <ArrowRight className="h-4 w-4 text-muted-foreground transform rotate-90" />
          </div>
        )}
        {confirmedNotifications.length > 0 && (
          <div className="border rounded-md bg-muted/30 p-3 space-y-2">
            <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              {t('timeline.confirmed')}{transaction.confirmed_at ? ` - ${formatDateTime(transaction.confirmed_at)}` : ''}
            </h5>
            {renderNotificationGroup(confirmedNotifications, "")}
          </div>
        )}
      </div>
    )
  }

  return (
    <Card className="mb-2">
      <CardContent className="p-0">
        <div className="px-3 py-1 -my-1">
        <div
          className="cursor-pointer"
          onClick={() => setIsExpanded(!isExpanded)}
        >
          {/* Header Row */}
          <div className="flex items-start justify-between mb-0.5">
            <div className="flex items-center gap-2">
              <Badge
                variant={transaction.transaction_status === "replaced" ? "secondary" : "outline"}
                className="text-xs"
              >
                {transaction.transaction_status === "replaced" ? (
                  <>
                    <XCircle className="h-3 w-3 text-orange-500 mr-1" />
                    {t('status.replaced')}
                  </>
                ) : transaction.block_height !== null ? (
                  <>
                    <CheckCircle className="h-3 w-3 text-green-500 mr-1" />
                    {transaction.transaction_type === "receive" ? t('types.receive') : t('types.send')}
                  </>
                ) : (
                  <>
                    <Loader2 className="h-3 w-3 text-yellow-500 animate-spin mr-1" />
                    {transaction.transaction_type === "receive" ? t('types.receiving') : t('types.sending')}
                  </>
                )}
              </Badge>
              {transaction.parent_txid && (
                <span title={t('tooltips.cpfp')}>
                  <Baby className="h-4 w-4" />
                </span>
              )}
              {transaction.replaced_by_txid && (
                <span title={t('tooltips.rbfReplaced')}>
                  <ArrowRight className="h-4 w-4 text-orange-500" />
                </span>
              )}
            </div>
            <div className="flex items-center gap-1">
              {isExpanded ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground" />
              )}
            </div>
          </div>

          {/* Amount and Date Row */}
          <div className="flex items-center justify-between">
            <div className="font-mono text-lg font-semibold">
              {formatTransactionAmount(transaction.amount_sats, transaction.transaction_type)}
            </div>
            <div className="text-sm text-muted-foreground flex items-center gap-1">
              <Clock className="h-3 w-3" />
              {formatDateTime(Math.min(
                transaction.first_seen_at,
                transaction.confirmed_at || Infinity
              ))}
            </div>
          </div>

          {/* Wallet Name (if multiple wallets) */}
          {showWalletName && (
            <div className="text-sm text-muted-foreground mt-1">
              {t('walletLabel', { name: transaction.wallet_name })}
            </div>
          )}

          </div>

        {/* Expanded Details */}
        {isExpanded && (
          <div className="mt-3 pt-3 border-t space-y-4">
            <div>
              <div className="space-y-1">
                <div className="flex items-center gap-3 text-sm">
                  <span className="font-medium min-w-[80px]">{t('details.txid')}:</span>
                  <a
                    href={`${mempoolBaseUrl}/tx/${transaction.txid}`}
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
                      href={`${mempoolBaseUrl}/tx/${transaction.replaced_by_txid}`}
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

            {/* Notification Details */}
            {renderNotificationDetails()}
          </div>
        )}
        </div>
      </CardContent>
    </Card>
  )
}
