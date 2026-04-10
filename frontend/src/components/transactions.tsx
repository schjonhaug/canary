"use client"

import React, { useEffect, useMemo, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { NotificationStatus, Transaction } from "../types"
import { TransactionCard } from "./transaction-card"
import { TransactionDetails } from "./transaction-details"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import {
  ArrowRight,
  Baby,
  Bell,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Loader2,
  Mail,
  MessageCircle,
  XCircle,
} from "lucide-react"
interface TransactionsProps {
  selectedWalletChecksum?: string | null
  transactions: Transaction[]
  error: string | null
  lastUpdate: number | null
  hasMoreTransactions?: boolean
  isLoadingMore?: boolean
  onLoadMore?: () => void
  walletsCount?: number
  transactionNotifications?: Record<string, NotificationStatus[]>
  loadingTransactionNotifications?: Record<string, boolean>
  transactionNotificationErrors?: Record<string, string | null>
  loadTransactionNotifications?: (walletChecksum: string, txid: string) => void
}

function getTransactionRowKey(transaction: Transaction) {
  return `${transaction.wallet_checksum}:${transaction.txid}`
}

function getTransactionDetailsId(transaction: Transaction) {
  return `transaction-details-${getTransactionRowKey(transaction)}`
}

function getDisplayTimestamp(transaction: Transaction) {
  if (transaction.confirmed_at === null) {
    return transaction.first_seen_at
  }

  return Math.min(transaction.first_seen_at, transaction.confirmed_at)
}

function normalizeProviderType(providerType?: string | null, providerName?: string) {
  return (providerType || providerName || "ntfy").toLowerCase()
}

export function Transactions({
  selectedWalletChecksum,
  transactions,
  error,
  lastUpdate,
  hasMoreTransactions = false,
  isLoadingMore = false,
  onLoadMore,
  walletsCount = 0,
  transactionNotifications = {},
  loadingTransactionNotifications = {},
  transactionNotificationErrors = {},
  loadTransactionNotifications = () => {},
}: TransactionsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set())
  const t = useTranslations("transactions")
  const tCommon = useTranslations("common")
  const { formatTransactionAmount, formatDateTime } = useFormatters()
  const loadOlderLabel = t("loadOlder")

  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  const filteredTransactions = useMemo(
    () =>
      selectedWalletChecksum
        ? transactions.filter(
            (transaction) => transaction.wallet_checksum === selectedWalletChecksum,
          )
        : transactions,
    [selectedWalletChecksum, transactions],
  )
  const transactionsByRowKey = useMemo(
    () =>
      new Map(
        filteredTransactions.map((transaction) => [
          getTransactionRowKey(transaction),
          transaction,
        ]),
      ),
    [filteredTransactions],
  )

  const getUniqueProviderSummary = (notifications?: NotificationStatus[]) => {
    if (!notifications || notifications.length === 0) return null

    const providerCounts = notifications.reduce((acc, notification) => {
      const providerType = normalizeProviderType(
        notification.provider_type,
        notification.provider_name,
      )
      acc[providerType] = (acc[providerType] || 0) + 1
      return acc
    }, {} as Record<string, number>)

    const getProviderIcon = (providerType: string) => {
      switch (providerType) {
        case "email":
          return <Mail className="h-4 w-4" />
        case "sms":
        case "twilio":
          return <MessageCircle className="h-4 w-4" />
        case "ntfy":
        default:
          return <Bell className="h-4 w-4" />
      }
    }

    const sortedProviderTypes = Object.keys(providerCounts).sort((a, b) => {
      const order = { email: 1, sms: 2, twilio: 2, ntfy: 3 }
      const aOrder = order[a as keyof typeof order] || 99
      const bOrder = order[b as keyof typeof order] || 99
      return aOrder - bOrder
    })

    return {
      icons: sortedProviderTypes.map((providerType) => ({
        icon: getProviderIcon(providerType),
        type: providerType,
      })),
    }
  }

  useEffect(() => {
    for (const rowKey of expandedRows) {
      const transaction = transactionsByRowKey.get(rowKey)
      if (transaction) {
        loadTransactionNotifications(transaction.wallet_checksum, transaction.txid)
      }
    }
  }, [expandedRows, lastUpdate, loadTransactionNotifications, transactionsByRowKey])

  const toggleRowExpansion = (transaction: Transaction) => {
    const rowKey = getTransactionRowKey(transaction)

    setExpandedRows((prev) => {
      const next = new Set(prev)
      if (next.has(rowKey)) {
        next.delete(rowKey)
      } else {
        next.add(rowKey)
      }
      return next
    })
  }

  const getCardTitle = () => {
    if (selectedWalletChecksum && filteredTransactions.length > 0) {
      const walletName =
        filteredTransactions[0]?.wallet_name || `Wallet ${selectedWalletChecksum}`
      return t("titleWithWallet", { walletName })
    }
    return t("title")
  }

  const getCardDescription = () => {
    if (selectedWalletChecksum && filteredTransactions.length === 0) {
      return t("emptyForWallet")
    }

    return undefined
  }

  const loadedCountLabel = useMemo(
    () => t("count", { count: filteredTransactions.length }),
    [filteredTransactions.length, t],
  )
  const cardDescription = getCardDescription()

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>{t("loading")}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="block space-y-3 md:hidden">
            {[1, 2, 3, 4, 5].map((item) => (
              <Card key={item} className="p-4">
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

          <div className="hidden md:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("tableHeaders.dateTime")}</TableHead>
                  {walletsCount > 1 && <TableHead>{t("tableHeaders.wallet")}</TableHead>}
                  <TableHead>{t("tableHeaders.transaction")}</TableHead>
                  <TableHead>{t("tableHeaders.amount")}</TableHead>
                  <TableHead className="w-8"></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {[1, 2, 3, 4, 5].map((item) => (
                  <TableRow key={item}>
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
                      <Skeleton className="h-4 w-4" />
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
          <CardTitle>{t("title")}</CardTitle>
          <CardDescription className="text-destructive">
            {t("error", { error })}
          </CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{getCardTitle()}</CardTitle>
        {cardDescription && <CardDescription>{cardDescription}</CardDescription>}
      </CardHeader>
      <CardContent className="space-y-4">
        {filteredTransactions.length === 0 ? (
          <p className="text-muted-foreground">
            {selectedWalletChecksum ? t("emptyForWallet") : t("empty")}
          </p>
        ) : (
          <>
            <div className="flex items-center justify-between text-sm text-muted-foreground">
              <span>{loadedCountLabel}</span>
            </div>

            <div className="block md:hidden">
              {filteredTransactions.map((transaction) => {
                const rowKey = getTransactionRowKey(transaction)
                const isExpanded = expandedRows.has(rowKey)

                return (
                  <TransactionCard
                    key={rowKey}
                    transaction={transaction}
                    showWalletName={walletsCount > 1}
                    isExpanded={isExpanded}
                    notifications={transactionNotifications[rowKey]}
                    isLoadingNotifications={loadingTransactionNotifications[rowKey]}
                    notificationError={transactionNotificationErrors[rowKey]}
                    onToggle={toggleRowExpansion}
                  />
                )
              })}
            </div>

            {hasMoreTransactions && onLoadMore && (
              <div className="mt-4 flex justify-center md:hidden">
                <Button variant="outline" onClick={onLoadMore} disabled={isLoadingMore}>
                  {isLoadingMore ? tCommon("loading") : loadOlderLabel}
                </Button>
              </div>
            )}

            <div className="hidden md:block">
              <Table>
                <TableCaption>{loadedCountLabel}</TableCaption>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("tableHeaders.dateTime")}</TableHead>
                    {walletsCount > 1 && <TableHead>{t("tableHeaders.wallet")}</TableHead>}
                    <TableHead>{t("tableHeaders.transaction")}</TableHead>
                    <TableHead>{t("tableHeaders.amount")}</TableHead>
                    <TableHead className="w-8"></TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {filteredTransactions.map((transaction) => {
                    const rowKey = getTransactionRowKey(transaction)
                    const isExpanded = expandedRows.has(rowKey)
                    const detailsId = getTransactionDetailsId(transaction)
                    const notificationSummary = getUniqueProviderSummary(
                      transactionNotifications[rowKey],
                    )

                    return (
                      <React.Fragment key={rowKey}>
                        <TableRow
                          role="button"
                          tabIndex={0}
                          aria-controls={detailsId}
                          aria-expanded={isExpanded}
                          className={`cursor-pointer ${isExpanded ? "bg-muted/30" : ""}`}
                          onClick={() => toggleRowExpansion(transaction)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault()
                              toggleRowExpansion(transaction)
                            }
                          }}
                        >
                          <TableCell className="text-sm">
                            {formatDateTime(getDisplayTimestamp(transaction))}
                          </TableCell>
                          {walletsCount > 1 && (
                            <TableCell className="font-medium">{transaction.wallet_name}</TableCell>
                          )}
                          <TableCell>
                            <div className="flex items-center gap-1">
                              <Badge
                                variant={
                                  transaction.transaction_status === "replaced"
                                    ? "secondary"
                                    : "outline"
                                }
                                className="flex items-center gap-1"
                                title={`${transaction.transaction_type === "receive" ? t("types.receive") : t("types.send")} - ${
                                  transaction.transaction_status === "replaced"
                                    ? t("tooltips.rbfReplaced")
                                    : transaction.block_height !== null
                                      ? t("status.confirmed")
                                      : t("status.pending")
                                }`}
                              >
                                {transaction.transaction_status === "replaced" ? (
                                  <XCircle className="h-3 w-3 text-orange-500" />
                                ) : transaction.block_height !== null ? (
                                  <CheckCircle className="h-3 w-3 text-green-500" />
                                ) : (
                                  <Loader2 className="h-3 w-3 animate-spin text-yellow-500" />
                                )}
                                {transaction.transaction_status === "replaced"
                                  ? t("status.replaced")
                                    : transaction.block_height !== null
                                      ? transaction.transaction_type === "receive"
                                        ? t("types.receive")
                                        : t("types.send")
                                      : transaction.transaction_type === "receive"
                                        ? t("types.receiving")
                                        : t("types.sending")}
                              </Badge>
                              {transaction.parent_txid && (
                                <span
                                  title={t("tooltips.cpfpChild", {
                                    txid: transaction.parent_txid,
                                  })}
                                >
                                  <Baby className="h-4 w-4" />
                                </span>
                              )}
                              {transaction.replaced_by_txid && (
                                <span
                                  title={t("tooltips.replacedByTx", {
                                    txid: transaction.replaced_by_txid,
                                  })}
                                >
                                  <ArrowRight className="h-4 w-4 text-orange-500" />
                                </span>
                              )}
                              {notificationSummary && (
                                <span className="ml-1 flex items-center gap-1 text-muted-foreground">
                                  {notificationSummary.icons.map((icon) => (
                                    <span key={icon.type}>{icon.icon}</span>
                                  ))}
                                </span>
                              )}
                            </div>
                          </TableCell>
                          <TableCell className="font-mono">
                            {formatTransactionAmount(
                              transaction.amount_sats,
                              transaction.transaction_type,
                            )}
                          </TableCell>
                          <TableCell className="text-center">
                            {isExpanded ? (
                              <ChevronDown className="h-4 w-4" />
                            ) : (
                              <ChevronRight className="h-4 w-4" />
                            )}
                          </TableCell>
                        </TableRow>
                        {isExpanded && (
                          <TableRow className="bg-muted/20">
                            <TableCell colSpan={walletsCount > 1 ? 5 : 4} className="p-0">
                              <div id={detailsId}>
                                <TransactionDetails
                                  transaction={transaction}
                                  isExpanded={isExpanded}
                                  notifications={transactionNotifications[rowKey]}
                                  isLoadingNotifications={loadingTransactionNotifications[rowKey]}
                                  notificationError={transactionNotificationErrors[rowKey]}
                                />
                              </div>
                            </TableCell>
                          </TableRow>
                        )}
                      </React.Fragment>
                    )
                  })}
                </TableBody>
              </Table>

              {hasMoreTransactions && onLoadMore && (
                <div className="mt-4 hidden justify-center md:flex">
                  <Button variant="outline" onClick={onLoadMore} disabled={isLoadingMore}>
                    {isLoadingMore ? tCommon("loading") : loadOlderLabel}
                  </Button>
                </div>
              )}
            </div>
          </>
        )}
      </CardContent>
    </Card>
  )
}
