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
import { CheckCircle, Clock, HandCoins, Baby, Mail, MessageCircle, Bell, ChevronDown, CheckCircle2, XCircle } from "lucide-react"
import { Transaction } from "../types"
import { formatBitcoinAmount, formatDateTime } from "@/lib/utils"

interface TransactionEventsProps {
  selectedWalletChecksum?: string | null
  transactions: Transaction[]
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
  walletsCount?: number
}

export function TransactionEvents({ selectedWalletChecksum, transactions, error, lastUpdate, walletsCount = 0 }: TransactionEventsProps) {
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
    return "Bitcoin transactions from all wallets"
  }

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>Loading transactions...</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Date/Time</TableHead>
                {walletsCount > 1 && <TableHead>Wallet</TableHead>}
                <TableHead>Transaction</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Total Balance</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {[1, 2, 3, 4, 5].map((i) => (
                <TableRow key={i}>
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
                    <Skeleton className="h-4 w-20" />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
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
          <Table>
            <TableCaption>A list of all transactions from the Canary system.</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>Date/Time</TableHead>
                {walletsCount > 1 && <TableHead>Wallet</TableHead>}
                <TableHead>Transaction</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Total Balance</TableHead>
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
                      <TableCell className="text-sm">
                        {formatDateTime(transaction.confirmed_at || transaction.first_seen_at)}
                      </TableCell>
                      {walletsCount > 1 && (
                        <TableCell className="font-medium">{transaction.wallet_name}</TableCell>
                      )}
                      <TableCell>
                        <div className="flex items-center gap-1">
                          <Badge 
                            variant="outline"
                            className="flex items-center gap-1"
                            title={`${transaction.transaction_type === "receive" ? "Receive" : "Send"} - ${transaction.block_height !== null ? "Confirmed" : "Pending"}`}
                          >
                            {transaction.block_height !== null ? (
                              <CheckCircle className="h-3 w-3 text-green-500" />
                            ) : (
                              <Clock className="h-3 w-3 text-yellow-500" />
                            )}
                            {transaction.block_height !== null 
                              ? (transaction.transaction_type === "receive" ? "Received" : "Sent")
                              : (transaction.transaction_type === "receive" ? "Receiving" : "Sending")
                            }
                          </Badge>
                          {transaction.is_cpfp && (
                            <span title="Child-Pays-For-Parent (CPFP)">
                              <Baby className="h-4 w-4 ml-1" />
                            </span>
                          )}
                          {transaction.is_rbf && (
                            <span title="Replace-By-Fee (RBF)">
                              <HandCoins className="h-4 w-4 ml-1" />
                            </span>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="font-mono">
                        {formatBitcoinAmount(transaction.amount_sats, transaction.transaction_type)}
                      </TableCell>
                      <TableCell className="font-mono">
                        {transaction.balance_after !== null && transaction.balance_after !== undefined ? formatBitcoinAmount(transaction.balance_after) : "N/A"}
                      </TableCell>
                      <TableCell className="text-sm">
                        {notificationSummary ? (
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-1">
                              {notificationSummary.icons.map((iconInfo, idx) => (
                                <span key={idx} title={`${iconInfo.count} ${iconInfo.type} notification${iconInfo.count !== 1 ? 's' : ''}`}>
                                  {iconInfo.icon}
                                </span>
                              ))}
                            </div>
                            <ChevronDown className={`h-4 w-4 transition-transform duration-200 ${isExpanded ? 'rotate-180' : ''}`} />
                          </div>
                        ) : (
                          <span>None</span>
                        )}
                      </TableCell>
                    </TableRow>
                    {isExpanded && transaction.notification_status && transaction.notification_status.length > 0 && (
                      <TableRow className="bg-muted/20">
                        <TableCell colSpan={walletsCount > 1 ? 6 : 5} className="p-0">
                          <div className="px-4 py-3">
                            <h4 className="text-sm font-medium mb-2">Notification Details</h4>
                            <div className="space-y-2">
                              {transaction.notification_status.map((notification, idx) => (
                                <div key={idx} className="flex items-center gap-3 text-sm">
                                  <span className="font-medium min-w-[100px]">{notification.contact_name}</span>
                                  <span className="text-muted-foreground">→</span>
                                  <span className="font-mono text-xs">
                                    {notification.notification_target || 'Unknown target'}
                                  </span>
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
                                  </div>
                                  <div className="flex items-center gap-1">
                                    {notification.status === 'sent' || notification.status === 'delivered' ? (
                                      <CheckCircle2 className="h-3 w-3 text-green-500" />
                                    ) : (
                                      <XCircle className="h-3 w-3 text-red-500" />
                                    )}
                                    <span className={`text-xs ${notification.status === 'sent' || notification.status === 'delivered' ? 'text-green-600' : 'text-red-600'}`}>
                                      {notification.status}
                                    </span>
                                  </div>
                                  {notification.error_message && (
                                    <span className="text-xs text-red-600 ml-2">
                                      {notification.error_message}
                                    </span>
                                  )}
                                </div>
                              ))}
                            </div>
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </React.Fragment>
                )
              })}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}