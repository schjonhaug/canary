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
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { CheckCircle, Baby, ChevronDown, ChevronRight, XCircle, Loader2, ArrowRight } from "lucide-react"
import { Transaction } from "../types"
import { TransactionCard } from "./transaction-card"
import { TransactionDetails } from "./transaction-details"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"

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
  const t = useTranslations('transactions')
  const tCommon = useTranslations('common')
  const { formatTransactionAmount, formatDateTime } = useFormatters()

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

              {hasMoreTransactions && onLoadMore && (
                <div className="mt-4 flex justify-center">
                  <Button variant="outline" onClick={onLoadMore} disabled={isLoadingMore}>
                    {isLoadingMore ? tCommon('loading') : tCommon('next')}
                  </Button>
                </div>
              )}
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
                      <TableCell colSpan={walletsCount > 1 ? 5 : 4} className={`overflow-hidden transition-all duration-300 ease-out ${isExpanded ? 'p-0' : 'p-0 h-0'}`}>
                        <TransactionDetails transaction={transaction} isExpanded={isExpanded} />
                      </TableCell>
                    </TableRow>
                  </React.Fragment>
                )
              })}
            </TableBody>
              </Table>
            </div>

            {hasMoreTransactions && onLoadMore && (
              <div className="mt-4 flex justify-center">
                <Button variant="outline" onClick={onLoadMore} disabled={isLoadingMore}>
                  {isLoadingMore ? tCommon('loading') : tCommon('next')}
                </Button>
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  )
}
