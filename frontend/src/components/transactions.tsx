"use client"

import React, { useEffect, useMemo, useRef, useState } from "react"
import { useVirtualizer } from "@tanstack/react-virtual"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Transaction } from "../types"
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
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
  hasMoreTransactions?: boolean
  isLoadingMore?: boolean
  onLoadMore?: () => void
  walletsCount?: number
}

function getSortLabel(transaction: Transaction) {
  return Math.min(transaction.first_seen_at, transaction.confirmed_at || Infinity)
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
}: TransactionsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set())
  const parentRef = useRef<HTMLDivElement | null>(null)
  const t = useTranslations("transactions")
  const tCommon = useTranslations("common")
  const { formatTransactionAmount, formatDateTime } = useFormatters()
  const loadOlderLabel = "Load older"

  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])

  const toggleRowExpansion = (txid: string) => {
    setExpandedRows((prev) => {
      const next = new Set(prev)
      if (next.has(txid)) {
        next.delete(txid)
      } else {
        next.add(txid)
      }
      return next
    })
  }

  const getUniqueProviderSummary = (
    notifications: Transaction["notification_status"],
  ) => {
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

  const filteredTransactions = selectedWalletChecksum
    ? transactions.filter(
        (transaction) => transaction.wallet_checksum === selectedWalletChecksum,
      )
    : transactions

  const rowVirtualizer = useVirtualizer({
    count: filteredTransactions.length,
    getScrollElement: () => parentRef.current,
    getItemKey: (index) => filteredTransactions[index]?.txid ?? index,
    estimateSize: (index) =>
      expandedRows.has(filteredTransactions[index]?.txid) ? 280 : 74,
    overscan: 8,
  })

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

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>{t("loading")}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="block space-y-3 md:hidden">
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

          <div className="hidden md:block space-y-2">
            {[1, 2, 3, 4, 5].map((i) => (
              <div
                key={i}
                className={`grid items-center gap-3 rounded-md border px-4 py-3 ${
                  walletsCount > 1
                    ? "grid-cols-[180px_140px_minmax(0,1fr)_140px_32px]"
                    : "grid-cols-[180px_minmax(0,1fr)_140px_32px]"
                }`}
              >
                <Skeleton className="h-4 w-28" />
                {walletsCount > 1 && <Skeleton className="h-4 w-24" />}
                <Skeleton className="h-6 w-28" />
                <Skeleton className="h-4 w-24" />
                <Skeleton className="h-4 w-4" />
              </div>
            ))}
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
        {getCardDescription() && (
          <CardDescription>{getCardDescription()}</CardDescription>
        )}
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

            <div className="block md:hidden space-y-2">
              {filteredTransactions.map((transaction) => (
                <TransactionCard
                  key={transaction.txid}
                  transaction={transaction}
                  showWalletName={walletsCount > 1}
                />
              ))}

              {hasMoreTransactions && onLoadMore && (
                <div className="mt-4 flex justify-center">
                  <Button variant="outline" onClick={onLoadMore} disabled={isLoadingMore}>
                    {isLoadingMore ? tCommon("loading") : loadOlderLabel}
                  </Button>
                </div>
              )}
            </div>

            <div className="hidden md:block space-y-2">
              <div
                className={`grid items-center gap-3 rounded-md border bg-muted/30 px-4 py-3 text-xs font-medium uppercase tracking-wide text-muted-foreground ${
                  walletsCount > 1
                    ? "grid-cols-[180px_140px_minmax(0,1fr)_140px_32px]"
                    : "grid-cols-[180px_minmax(0,1fr)_140px_32px]"
                }`}
              >
                <span>{t("tableHeaders.dateTime")}</span>
                {walletsCount > 1 && <span>{t("tableHeaders.wallet")}</span>}
                <span>{t("tableHeaders.transaction")}</span>
                <span>{t("tableHeaders.amount")}</span>
                <span />
              </div>

              <div
                ref={parentRef}
                className="max-h-[70vh] overflow-auto rounded-md border"
              >
                <div
                  className="relative w-full"
                  style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
                >
                  {rowVirtualizer.getVirtualItems().map((virtualRow) => {
                    const transaction = filteredTransactions[virtualRow.index]
                    const isExpanded = expandedRows.has(transaction.txid)
                    const detailsId = `transaction-details-${transaction.txid}`
                    const notificationSummary = getUniqueProviderSummary(
                      transaction.notification_status,
                    )

                    return (
                      <div
                        key={transaction.txid}
                        ref={rowVirtualizer.measureElement}
                        data-index={virtualRow.index}
                        className="absolute left-0 top-0 w-full border-b bg-card"
                        style={{
                          transform: `translateY(${virtualRow.start}px)`,
                        }}
                      >
                        <button
                          type="button"
                          aria-controls={detailsId}
                          aria-expanded={isExpanded}
                          className={`grid w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/50 ${
                            walletsCount > 1
                              ? "grid-cols-[180px_140px_minmax(0,1fr)_140px_32px]"
                              : "grid-cols-[180px_minmax(0,1fr)_140px_32px]"
                          } ${isExpanded ? "bg-muted/30" : ""}`}
                          onClick={() => toggleRowExpansion(transaction.txid)}
                        >
                          <span className="text-sm">
                            {formatDateTime(getSortLabel(transaction))}
                          </span>

                          {walletsCount > 1 && (
                            <span className="font-medium">{transaction.wallet_name}</span>
                          )}

                          <span className="flex min-w-0 items-center gap-2">
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
                          </span>

                          <span className="font-mono">
                            {formatTransactionAmount(
                              transaction.amount_sats,
                              transaction.transaction_type,
                            )}
                          </span>

                          <span className="flex justify-center">
                            {isExpanded ? (
                              <ChevronDown className="h-4 w-4" />
                            ) : (
                              <ChevronRight className="h-4 w-4" />
                            )}
                          </span>
                        </button>

                        {isExpanded && (
                          <div id={detailsId}>
                            <TransactionDetails
                              transaction={transaction}
                              isExpanded={isExpanded}
                            />
                          </div>
                        )}
                      </div>
                    )
                  })}
                </div>
              </div>

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
