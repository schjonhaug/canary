"use client"

import React, { useEffect, useState } from "react"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { CheckCircle, Baby, Mail, MessageCircle, Bell, ChevronDown, ChevronRight, XCircle, Loader2, ArrowRight } from "lucide-react"
import { Transaction } from "../types"
import { formatBitcoinAmount, formatTransactionAmount, formatDateTime } from "@/lib/utils"
import { TransactionCard } from "./transaction-card"
import { useTranslations } from "next-intl"

interface TransactionsProps {
  selectedWalletChecksum?: string | null
  transactions: Transaction[]
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
  walletsCount?: number
}

export function Transactions({ selectedWalletChecksum, transactions, error, lastUpdate, walletsCount = 0 }: TransactionsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set())
  const t = useTranslations('transactions')

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  // Toggle row expansion
  const toggleRowExpansion = (eventId: string) => {
    setExpandedRows(prev => {
      const newSet = new Set(prev)
      if (newSet.has(eventId)) {
        newSet.delete(eventId)
      } else {
        newSet.add(eventId)
      }
      return newSet
    })
  }

  // Get unique provider types and icons for condensed view
  const getUniqueProviderSummary = (notifications: typeof transactions[0]['notification_status']) => {
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

    // Sort provider types for consistent order: email, sms, ntfy
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


  // Filter transactions by selected wallet if one is selected
  const filteredTransactions = selectedWalletChecksum 
    ? transactions.filter(transaction => transaction.wallet_checksum === selectedWalletChecksum)
    : transactions

  const getCardTitle = () => {
    if (selectedWalletChecksum && filteredTransactions.length > 0) {
      const walletName = filteredTransactions[0]?.wallet_name || `Wallet ${selectedWalletChecksum}`
      return t('titleWithWallet', { walletName })
    }
    return t('title')
  }

  const getCardDescription = () => {
    if (selectedWalletChecksum && filteredTransactions.length === 0) {
      return t('emptyForWallet')
    }
    return undefined
  }

  const getTableCaption = () => {
    const transactionCount = filteredTransactions.length
    return t('count', { count: transactionCount })
  }

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>{t('loading')}</CardDescription>
        </CardHeader>
        <CardContent>
          {/* Mobile Loading - Cards */}
          <div className="block md:hidden space-y-3">
            {[1, 2, 3, 4, 5].map((i) => (
              <Card key={i} className="p-4">
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <Skeleton className="h-6 w-20" />
                    <Skeleton className="h-4 w-4" />
                  </div>
                  <div className="flex items-center justify-between">
                    <Skeleton className="h-6 w-32" />
                    <Skeleton className="h-4 w-24" />
                  </div>
                  {walletsCount > 1 && <Skeleton className="h-4 w-28" />}
                  <Skeleton className="h-4 w-40" />
                </div>
              </Card>
            ))}
          </div>

          {/* Desktop Loading - Table */}
          <div className="hidden md:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8 hidden sm:table-cell"></TableHead>
                  <TableHead>{t('tableHeaders.dateTime')}</TableHead>
                  {walletsCount > 1 && <TableHead>{t('tableHeaders.wallet')}</TableHead>}
                  <TableHead>{t('tableHeaders.transaction')}</TableHead>
                  <TableHead>{t('tableHeaders.amount')}</TableHead>
                  <TableHead>{t('tableHeaders.details')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {[1, 2, 3, 4, 5].map((i) => (
                  <TableRow key={i}>
                    <TableCell className="hidden sm:table-cell">
                      <Skeleton className="h-4 w-4" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-32" />
                    </TableCell>
                    {walletsCount > 1 && (
                      <TableCell>
                        <Skeleton className="h-6 w-20" />
                      </TableCell>
                    )}
                    <TableCell>
                      <Skeleton className="h-4 w-28" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-28" />
                    </TableCell>
                    <TableCell>
                      <Skeleton className="h-4 w-8" />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>
    )
  }

  if (error && transactions.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('title')}</CardTitle>
          <CardDescription className="text-destructive">{t('error', { error })}</CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{getCardTitle()}</CardTitle>
        {getCardDescription() && <CardDescription>{getCardDescription()}</CardDescription>}
      </CardHeader>
      <CardContent>
        {filteredTransactions.length === 0 ? (
          <p className="text-muted-foreground">
            {selectedWalletChecksum
              ? t('emptyForWallet')
              : t('empty')
            }
          </p>
        ) : (
          <>
            {/* Mobile View - Cards (visible on screens smaller than 768px) */}
            <div className="block md:hidden">
              {filteredTransactions.map((transaction) => (
                <TransactionCard
                  key={transaction.txid}
                  transaction={transaction}
                  showWalletName={walletsCount > 1}
                />
              ))}
            </div>

            {/* Desktop View - Table (visible on screens 768px and larger) */}
            <div className="hidden md:block">
              <Table>
            <TableCaption>{getTableCaption()}</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>{t('tableHeaders.dateTime')}</TableHead>
                {walletsCount > 1 && <TableHead>{t('tableHeaders.wallet')}</TableHead>}
                <TableHead>{t('tableHeaders.transaction')}</TableHead>
                <TableHead>{t('tableHeaders.amount')}</TableHead>
                <TableHead className="w-8"></TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredTransactions.map((transaction) => {
                const isExpanded = expandedRows.has(transaction.txid)
                const notificationSummary = getUniqueProviderSummary(transaction.notification_status)
                
                return (
                  <React.Fragment key={transaction.txid}>
                    <TableRow 
                      className={`cursor-pointer hover:bg-muted/50 transition-colors ${isExpanded ? 'bg-muted/30' : ''}`}
                      onClick={() => toggleRowExpansion(transaction.txid)}
                    >
                      <TableCell className="text-sm">
                        {formatDateTime(Math.min(
                          transaction.first_seen_at,
                          transaction.confirmed_at || Infinity
                        ))}
                      </TableCell>
                      {walletsCount > 1 && (
                        <TableCell className="font-medium">{transaction.wallet_name}</TableCell>
                      )}
                      <TableCell>
                        <div className="flex items-center gap-1">
                          <Badge
                            variant={transaction.transaction_status === "replaced" ? "secondary" : "outline"}
                            className="flex items-center gap-1"
                            title={`${transaction.transaction_type === "receive" ? t('types.receive') : t('types.send')} - ${
                              transaction.transaction_status === "replaced" ? t('tooltips.rbfReplaced') :
                              transaction.block_height !== null ? t('status.confirmed') : t('status.pending')
                            }`}
                          >
                            {transaction.transaction_status === "replaced" ? (
                              <XCircle className="h-3 w-3 text-orange-500" />
                            ) : transaction.block_height !== null ? (
                              <CheckCircle className="h-3 w-3 text-green-500" />
                            ) : (
                              <Loader2 className="h-3 w-3 text-yellow-500 animate-spin" />
                            )}
                            {transaction.transaction_status === "replaced"
                              ? t('status.replaced')
                              : transaction.block_height !== null
                                ? (transaction.transaction_type === "receive" ? t('types.receive') : t('types.send'))
                                : (transaction.transaction_type === "receive" ? t('types.receiving') : t('types.sending'))
                            }
                          </Badge>
                          {transaction.parent_txid && (
                            <span title={t('tooltips.cpfpChild', { txid: transaction.parent_txid })}>
                              <Baby className="h-4 w-4 ml-1" />
                            </span>
                          )}
                          {transaction.replaced_by_txid && (
                            <span title={t('tooltips.replacedByTx', { txid: transaction.replaced_by_txid })}>
                              <ArrowRight className="h-4 w-4 ml-1 text-orange-500" />
                            </span>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="font-mono">
                        {formatTransactionAmount(transaction.amount_sats, transaction.transaction_type)}
                      </TableCell>
                      <TableCell className="text-center">
                        {isExpanded ? (
                          <ChevronDown className="h-4 w-4 transition-transform duration-200" />
                        ) : (
                          <ChevronRight className="h-4 w-4 transition-transform duration-200" />
                        )}
                      </TableCell>
                    </TableRow>
                    <TableRow className={`bg-muted/20 transition-all duration-300 ease-out overflow-hidden ${isExpanded ? 'h-auto' : 'h-0'}`} style={{ lineHeight: isExpanded ? 'normal' : '0' }}>
                      <TableCell colSpan={walletsCount > 1 ? 5 : 4} className={`overflow-hidden transition-all duration-300 ease-out ${isExpanded ? 'p-0' : 'p-0 h-0'} sm:hidden`}>
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
                                <div>
                                  {(() => {
                                    // Group notifications by type
                                    const groupedNotifications = transaction.notification_status.reduce((acc, notification) => {
                                      const type = notification.notification_type === 'pending' ? 'pending' : 'confirmed'
                                      if (!acc[type]) acc[type] = []
                                      acc[type].push(notification)
                                      return acc
                                    }, {} as Record<string, typeof transaction.notification_status>)

                                    const pendingNotifications = groupedNotifications.pending || []
                                    const confirmedNotifications = groupedNotifications.confirmed || []


                                    const renderNotificationGroup = (notifications: typeof transaction.notification_status) => {
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
                                                    <XCircle className="h-3 w-3 text-red-500 ml-1" />
                                                  </span>
                                                )}
                                              </div>
                                            </div>
                                          )
                                        })
                                    }

                                    return (
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
                                    )
                                  })()}
                              </div>
                            )}
                            </div>
                          </div>
                        </TableCell>
                      <TableCell colSpan={walletsCount > 1 ? 5 : 4} className={`overflow-hidden transition-all duration-300 ease-out ${isExpanded ? 'p-0' : 'p-0 h-0'} hidden sm:table-cell`}>
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
                                <div>
                                  {(() => {
                                    // Group notifications by type
                                    const groupedNotifications = transaction.notification_status.reduce((acc, notification) => {
                                      const type = notification.notification_type === 'pending' ? 'pending' : 'confirmed'
                                      if (!acc[type]) acc[type] = []
                                      acc[type].push(notification)
                                      return acc
                                    }, {} as Record<string, typeof transaction.notification_status>)

                                    const pendingNotifications = groupedNotifications.pending || []
                                    const confirmedNotifications = groupedNotifications.confirmed || []


                                    const renderNotificationGroup = (notifications: typeof transaction.notification_status) => {
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
                                                    <XCircle className="h-3 w-3 text-red-500 ml-1" />
                                                  </span>
                                                )}
                                              </div>
                                            </div>
                                          )
                                        })
                                    }

                                    return (
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
                                    )
                                  })()}
                              </div>
                            )}
                            </div>
                          </div>
                        </TableCell>
                      </TableRow>
                  </React.Fragment>
                )
              })}
            </TableBody>
              </Table>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  )
}
