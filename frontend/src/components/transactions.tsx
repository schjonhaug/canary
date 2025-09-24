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
import { formatBitcoinAmount, formatDateTime } from "@/lib/utils"
import { TransactionCard } from "./transaction-card"

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
      return `Transactions - ${walletName}`
    }
    return "Transactions"
  }

  const getCardDescription = () => {
    if (selectedWalletChecksum) {
      return filteredTransactions.length > 0
        ? `${filteredTransactions.length} transaction${filteredTransactions.length !== 1 ? 's' : ''} for selected wallet`
        : "No transactions found for selected wallet"
    }
    return ""
  }

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>Loading transactions...</CardDescription>
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
                  <TableHead>Date/Time</TableHead>
                  {walletsCount > 1 && <TableHead>Wallet</TableHead>}
                  <TableHead>Transaction</TableHead>
                  <TableHead>Amount</TableHead>
                  <TableHead>Details</TableHead>
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
          <CardTitle>Transaction Events</CardTitle>
          <CardDescription className="text-destructive">Error: {error}</CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{getCardTitle()}</CardTitle>
        <CardDescription>{getCardDescription()}</CardDescription>
      </CardHeader>
      <CardContent>
        {filteredTransactions.length === 0 ? (
          <p className="text-muted-foreground">
            {selectedWalletChecksum
              ? "No transactions found for the selected wallet."
              : "No transactions found."
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
            <TableCaption>A list of all transactions from the Canary system.</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead className="w-8 hidden sm:table-cell"></TableHead>
                <TableHead>Date/Time</TableHead>
                {walletsCount > 1 && <TableHead>Wallet</TableHead>}
                <TableHead>Transaction</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Notifications</TableHead>
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
                      <TableCell className="text-center hidden sm:table-cell">
                        {isExpanded ? (
                          <ChevronDown className="h-4 w-4 transition-transform duration-200" />
                        ) : (
                          <ChevronRight className="h-4 w-4 transition-transform duration-200" />
                        )}
                      </TableCell>
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
                            title={`${transaction.transaction_type === "receive" ? "Receive" : "Send"} - ${
                              transaction.transaction_status === "replaced" ? "Replaced by RBF" :
                              transaction.block_height !== null ? "Confirmed" : "Pending"
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
                              ? "Replaced"
                              : transaction.block_height !== null 
                                ? (transaction.transaction_type === "receive" ? "Received" : "Sent")
                                : (transaction.transaction_type === "receive" ? "Receiving" : "Sending")
                            }
                          </Badge>
                          {transaction.parent_txid && (
                            <span title={`Child-Pays-For-Parent (CPFP) - Child of ${transaction.parent_txid}`}>
                              <Baby className="h-4 w-4 ml-1" />
                            </span>
                          )}
                          {transaction.replaced_by_txid && (
                            <span title={`Replaced by transaction: ${transaction.replaced_by_txid}`}>
                              <ArrowRight className="h-4 w-4 ml-1 text-orange-500" />
                            </span>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="font-mono">
                        {formatBitcoinAmount(transaction.amount_sats, transaction.transaction_type)}
                      </TableCell>
                      <TableCell className="text-sm">
                        {notificationSummary && (
                          <div className="flex items-center gap-1">
                            {notificationSummary.icons.map((iconInfo, idx) => (
                              <span key={idx} title={`${iconInfo.count} ${iconInfo.type} notification${iconInfo.count !== 1 ? 's' : ''}`}>
                                {iconInfo.icon}
                              </span>
                            ))}
                          </div>
                        )}
                      </TableCell>
                    </TableRow>
                    <TableRow className={`bg-muted/20 transition-all duration-300 ease-out overflow-hidden ${isExpanded ? 'h-auto' : 'h-0'}`} style={{ lineHeight: isExpanded ? 'normal' : '0' }}>
                      <TableCell colSpan={walletsCount > 1 ? 6 : 5} className={`overflow-hidden transition-all duration-300 ease-out ${isExpanded ? 'p-0' : 'p-0 h-0'} sm:hidden`}>
                        <div className={`px-4 transform transition-all duration-300 ease-out overflow-hidden ${isExpanded ? 'py-3 translate-y-0 max-h-96' : 'py-0 -translate-y-2 max-h-0'}`}>
                            <div className="space-y-4">
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
                                        .map(([contactName, contactNotifications]) => (
                                        <div key={contactName} className="space-y-1">
                                          {contactNotifications.map((notification, idx) => {
                                            const notificationTime = notification.created_at ? formatDateTime(notification.created_at) : 'Unknown time'
                                            return (
                                              <div
                                                key={idx}
                                                className="flex items-center gap-2 text-sm"
                                                title={`Notification sent at: ${notificationTime}`}
                                              >
                                                <span className="font-medium">{contactName}</span>
                                                <div className="flex items-center gap-1">
                                                {(() => {
                                                  const providerType = notification.provider_type || notification.provider_name.toLowerCase()
                                                  switch (providerType) {
                                                    case 'email':
                                                      return <Mail className="h-3 w-3" />
                                                    case 'sms':
                                                    case 'twilio':
                                                      return <MessageCircle className="h-3 w-3" />
                                                    case 'ntfy':
                                                    default:
                                                      return <Bell className="h-3 w-3" />
                                                  }
                                                })()}
                                                <span className="font-mono text-xs">
                                                  {notification.notification_target || 'Unknown target'}
                                                </span>
                                              </div>
                                              {/* Only show status if there's an error (not sent/delivered successfully) */}
                                              {notification.status !== 'sent' && notification.status !== 'delivered' && (
                                                <div className="flex items-center gap-1">
                                                  <XCircle className="h-3 w-3 text-red-500" />
                                                  <span className="text-xs text-red-600">
                                                    {notification.status}
                                                  </span>
                                                </div>
                                              )}
                                              {notification.error_message && (
                                                <span className="text-xs text-red-600 ml-2">
                                                  {notification.error_message}
                                                </span>
                                              )}
                                            </div>
                                            )
                                          })}
                                        </div>
                                      ))
                                    }

                                    return (
                                      <div className="space-y-3">
                                        {/* Headings Row */}
                                        <div className="flex items-center">
                                          <div className="flex-1">
                                            {pendingNotifications.length > 0 && (
                                              <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                                                PENDING - {formatDateTime(transaction.first_seen_at)}
                                              </h5>
                                            )}
                                          </div>
                                          {pendingNotifications.length > 0 && confirmedNotifications.length > 0 && (
                                            <div className="flex items-center justify-center px-4">
                                              <ArrowRight className="h-4 w-4 text-muted-foreground" />
                                            </div>
                                          )}
                                          <div className="flex-1 text-right">
                                            {confirmedNotifications.length > 0 && (
                                              <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                                                CONFIRMED{transaction.confirmed_at ? ` - ${formatDateTime(transaction.confirmed_at)}` : ''}
                                              </h5>
                                            )}
                                          </div>
                                        </div>

                                        {/* Content Row */}
                                        <div className="flex justify-between items-start gap-8">
                                          <div className="flex-1">
                                            {pendingNotifications.length > 0 && (
                                              <div className="space-y-2 ml-2">
                                                {renderNotificationGroup(pendingNotifications)}
                                              </div>
                                            )}
                                          </div>
                                          <div className="flex-1">
                                            {confirmedNotifications.length > 0 && (
                                              <div className="space-y-2 ml-2">
                                                {renderNotificationGroup(confirmedNotifications)}
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
                      <TableCell colSpan={walletsCount > 1 ? 7 : 6} className={`overflow-hidden transition-all duration-300 ease-out ${isExpanded ? 'p-0' : 'p-0 h-0'} hidden sm:table-cell`}>
                        <div className={`px-4 transform transition-all duration-300 ease-out overflow-hidden ${isExpanded ? 'py-3 translate-y-0 max-h-96' : 'py-0 -translate-y-2 max-h-0'}`}>
                            <div className="space-y-4">
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
                                        .map(([contactName, contactNotifications]) => (
                                        <div key={contactName} className="space-y-1">
                                          {contactNotifications.map((notification, idx) => {
                                            const notificationTime = notification.created_at ? formatDateTime(notification.created_at) : 'Unknown time'
                                            return (
                                              <div
                                                key={idx}
                                                className="flex items-center gap-2 text-sm"
                                                title={`Notification sent at: ${notificationTime}`}
                                              >
                                                <span className="font-medium">{contactName}</span>
                                                <div className="flex items-center gap-1">
                                                {(() => {
                                                  const providerType = notification.provider_type || notification.provider_name.toLowerCase()
                                                  switch (providerType) {
                                                    case 'email':
                                                      return <Mail className="h-3 w-3" />
                                                    case 'sms':
                                                    case 'twilio':
                                                      return <MessageCircle className="h-3 w-3" />
                                                    case 'ntfy':
                                                    default:
                                                      return <Bell className="h-3 w-3" />
                                                  }
                                                })()}
                                                <span className="font-mono text-xs">
                                                  {notification.notification_target || 'Unknown target'}
                                                </span>
                                              </div>
                                              {/* Only show status if there's an error (not sent/delivered successfully) */}
                                              {notification.status !== 'sent' && notification.status !== 'delivered' && (
                                                <div className="flex items-center gap-1">
                                                  <XCircle className="h-3 w-3 text-red-500" />
                                                  <span className="text-xs text-red-600">
                                                    {notification.status}
                                                  </span>
                                                </div>
                                              )}
                                              {notification.error_message && (
                                                <span className="text-xs text-red-600 ml-2">
                                                  {notification.error_message}
                                                </span>
                                              )}
                                            </div>
                                            )
                                          })}
                                        </div>
                                      ))
                                    }

                                    return (
                                      <div className="space-y-3">
                                        {/* Headings Row */}
                                        <div className="flex items-center">
                                          <div className="flex-1">
                                            {pendingNotifications.length > 0 && (
                                              <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                                                PENDING - {formatDateTime(transaction.first_seen_at)}
                                              </h5>
                                            )}
                                          </div>
                                          {pendingNotifications.length > 0 && confirmedNotifications.length > 0 && (
                                            <div className="flex items-center justify-center px-4">
                                              <ArrowRight className="h-4 w-4 text-muted-foreground" />
                                            </div>
                                          )}
                                          <div className="flex-1 text-right">
                                            {confirmedNotifications.length > 0 && (
                                              <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                                                CONFIRMED{transaction.confirmed_at ? ` - ${formatDateTime(transaction.confirmed_at)}` : ''}
                                              </h5>
                                            )}
                                          </div>
                                        </div>

                                        {/* Content Row */}
                                        <div className="flex justify-between items-start gap-8">
                                          <div className="flex-1">
                                            {pendingNotifications.length > 0 && (
                                              <div className="space-y-2 ml-2">
                                                {renderNotificationGroup(pendingNotifications)}
                                              </div>
                                            )}
                                          </div>
                                          <div className="flex-1">
                                            {confirmedNotifications.length > 0 && (
                                              <div className="space-y-2 ml-2">
                                                {renderNotificationGroup(confirmedNotifications)}
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