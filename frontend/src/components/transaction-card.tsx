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
import { formatBitcoinAmount, formatDateTime } from "@/lib/utils"

interface TransactionCardProps {
  transaction: Transaction
  showWalletName: boolean
}

export function TransactionCard({ transaction, showWalletName }: TransactionCardProps) {
  const [isExpanded, setIsExpanded] = useState(false)

  const getUniqueProviderSummary = (notifications: typeof transaction.notification_status) => {
    if (!notifications || notifications.length === 0) return null

    const providerCounts = notifications.reduce((acc, notification) => {
      const providerType = notification.provider_type || notification.provider_name.toLowerCase()
      acc[providerType] = (acc[providerType] || 0) + 1
      return acc
    }, {} as Record<string, number>)

    const getProviderIcon = (providerType: string) => {
      switch (providerType) {
        case 'email':
          return <Mail className="h-4 w-4" />
        case 'sms':
        case 'twilio':
          return <MessageCircle className="h-4 w-4" />
        case 'ntfy':
        default:
          return <Bell className="h-4 w-4" />
      }
    }

    const sortedProviderTypes = Object.keys(providerCounts).sort((a, b) => {
      const order = { 'email': 1, 'sms': 2, 'twilio': 2, 'ntfy': 3 }
      const aOrder = order[a as keyof typeof order] || 99
      const bOrder = order[b as keyof typeof order] || 99
      return aOrder - bOrder
    })

    return {
      icons: sortedProviderTypes.map(providerType => ({
        icon: getProviderIcon(providerType),
        count: providerCounts[providerType],
        type: providerType
      })),
      total: notifications.length
    }
  }

  const notificationSummary = getUniqueProviderSummary(transaction.notification_status)

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
          <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">{title}</h5>
          <div className="ml-2 space-y-1">
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
                        const notificationTime = notification.created_at ? formatDateTime(notification.created_at) : 'Unknown time'
                        const providerType = notification.provider_type || notification.provider_name.toLowerCase()
                        const target = notification.notification_target || 'Unknown target'
                        const hasError = notification.status !== 'sent' && notification.status !== 'delivered'

                        let tooltipText = `${target}\nSent at: ${notificationTime}`
                        if (hasError) {
                          tooltipText += `\nStatus: ${notification.status}`
                          if (notification.error_message) {
                            tooltipText += `\nError: ${notification.error_message}`
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
                        <span title="Some notifications failed">
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
          <div className="space-y-2">
            <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              PENDING - {formatDateTime(transaction.first_seen_at)}
            </h5>
            <div className="ml-2">
              {renderNotificationGroup(pendingNotifications, "Pending")}
            </div>
          </div>
        )}
        {confirmedNotifications.length > 0 && (
          <div className="space-y-2">
            <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              CONFIRMED{transaction.confirmed_at ? ` - ${formatDateTime(transaction.confirmed_at)}` : ''}
            </h5>
            <div className="ml-2">
              {renderNotificationGroup(confirmedNotifications, "Confirmed")}
            </div>
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
                    Replaced
                  </>
                ) : transaction.block_height !== null ? (
                  <>
                    <CheckCircle className="h-3 w-3 text-green-500 mr-1" />
                    {transaction.transaction_type === "receive" ? "Received" : "Sent"}
                  </>
                ) : (
                  <>
                    <Loader2 className="h-3 w-3 text-yellow-500 animate-spin mr-1" />
                    {transaction.transaction_type === "receive" ? "Receiving" : "Sending"}
                  </>
                )}
              </Badge>
              {transaction.parent_txid && (
                <span title="CPFP transaction">
                  <Baby className="h-4 w-4" />
                </span>
              )}
              {transaction.replaced_by_txid && (
                <span title="Replaced by RBF">
                  <ArrowRight className="h-4 w-4 text-orange-500" />
                </span>
              )}
            </div>
            <div className="flex items-center gap-1">
              {notificationSummary ? (
                notificationSummary.icons.map((iconInfo, idx) => (
                  <span key={idx} title={`${iconInfo.count} ${iconInfo.type} notification${iconInfo.count !== 1 ? 's' : ''}`}>
                    {iconInfo.icon}
                  </span>
                ))
              ) : null}
              {isExpanded ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground ml-1" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground ml-1" />
              )}
            </div>
          </div>

          {/* Amount and Date Row */}
          <div className="flex items-center justify-between">
            <div className="font-mono text-lg font-semibold">
              {formatBitcoinAmount(transaction.amount_sats, transaction.transaction_type)}
            </div>
            <div className="text-sm text-muted-foreground flex items-center gap-1">
              <Clock className="h-3 w-3" />
              {(() => {
                // Use the oldest available timestamp
                const dateTime = Math.min(
                  transaction.first_seen_at,
                  transaction.confirmed_at || Infinity
                )
                const date = new Date(dateTime * 1000)

                return date.toLocaleDateString(undefined, {
                  year: '2-digit',
                  month: '2-digit',
                  day: '2-digit'
                })
              })()}
            </div>
          </div>

          {/* Wallet Name (if multiple wallets) */}
          {showWalletName && (
            <div className="text-sm text-muted-foreground mt-1">
              Wallet: <span className="font-medium">{transaction.wallet_name}</span>
            </div>
          )}

          </div>

        {/* Expanded Details */}
        {isExpanded && (
          <div className="mt-3 pt-3 border-t space-y-4">
            {/* Transaction Details */}
            <div>
              <h4 className="text-sm font-medium mb-2">Transaction Details</h4>
              <div className="space-y-1 ml-2">
                <div className="flex items-center gap-3 text-sm">
                  <span className="font-medium min-w-[80px]">Transaction ID:</span>
                  <span className="font-mono text-xs break-all">{transaction.txid}</span>
                </div>
                {transaction.fee_sats && (
                  <div className="flex items-center gap-3 text-sm">
                    <span className="font-medium min-w-[80px]">Fee:</span>
                    <span className="font-mono text-xs">{formatBitcoinAmount(transaction.fee_sats)}</span>
                  </div>
                )}
                {transaction.transaction_status === "replaced" && transaction.replaced_by_txid && (
                  <div className="flex items-center gap-3 text-sm">
                    <span className="font-medium min-w-[80px]">Replaced by:</span>
                    <span className="font-mono text-xs break-all text-orange-600">{transaction.replaced_by_txid}</span>
                  </div>
                )}
                {transaction.replaced_at && (
                  <div className="flex items-center gap-3 text-sm">
                    <span className="font-medium min-w-[80px]">Replaced at:</span>
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
                    PENDING - {formatDateTime(transaction.first_seen_at)}
                  </span>
                  {transaction.confirmed_at && (
                    <>
                      <ArrowRight className="h-3 w-3" />
                      <span className="font-semibold">
                        CONFIRMED - {formatDateTime(transaction.confirmed_at)}
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