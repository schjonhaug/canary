"use client"

import React from "react"
import { Card, CardContent } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import {
  ArrowRight,
  Baby,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Clock,
  Loader2,
  XCircle,
} from "lucide-react"
import { NotificationStatus, Transaction } from "../types"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import { TransactionDetails } from "./transaction-details"

interface TransactionCardProps {
  transaction: Transaction
  showWalletName: boolean
  isExpanded: boolean
  notifications?: NotificationStatus[]
  isLoadingNotifications?: boolean
  notificationError?: string | null
  onToggle: (transaction: Transaction) => void
}

export function TransactionCard({
  transaction,
  showWalletName,
  isExpanded,
  notifications,
  isLoadingNotifications = false,
  notificationError = null,
  onToggle,
}: TransactionCardProps) {
  const t = useTranslations("transactions")
  const { formatTransactionAmount, formatDateTime } = useFormatters()

  return (
    <Card className="mb-2">
      <CardContent className="p-0">
        <div className="px-3 py-1 -my-1">
          <div className="cursor-pointer" onClick={() => onToggle(transaction)}>
            <div className="mb-0.5 flex items-start justify-between">
              <div className="flex items-center gap-2">
                <Badge
                  variant={transaction.transaction_status === "replaced" ? "secondary" : "outline"}
                  className="text-xs"
                >
                  {transaction.transaction_status === "replaced" ? (
                    <>
                      <XCircle className="mr-1 h-3 w-3 text-orange-500" />
                      {t("status.replaced")}
                    </>
                  ) : transaction.block_height !== null ? (
                    <>
                      <CheckCircle className="mr-1 h-3 w-3 text-green-500" />
                      {transaction.transaction_type === "receive"
                        ? t("types.receive")
                        : t("types.send")}
                    </>
                  ) : (
                    <>
                      <Loader2 className="mr-1 h-3 w-3 animate-spin text-yellow-500" />
                      {transaction.transaction_type === "receive"
                        ? t("types.receiving")
                        : t("types.sending")}
                    </>
                  )}
                </Badge>
                {transaction.parent_txid && (
                  <span title={t("tooltips.cpfp")}>
                    <Baby className="h-4 w-4" />
                  </span>
                )}
                {transaction.replaced_by_txid && (
                  <span title={t("tooltips.rbfReplaced")}>
                    <ArrowRight className="h-4 w-4 text-orange-500" />
                  </span>
                )}
              </div>
              {isExpanded ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground" />
              )}
            </div>

            <div className="flex items-center justify-between">
              <div className="font-mono text-lg font-semibold">
                {formatTransactionAmount(transaction.amount_sats, transaction.transaction_type)}
              </div>
              <div className="flex items-center gap-1 text-sm text-muted-foreground">
                <Clock className="h-3 w-3" />
                {formatDateTime(
                  Math.min(transaction.first_seen_at, transaction.confirmed_at || Infinity)
                )}
              </div>
            </div>

            {showWalletName && (
              <div className="mt-1 text-sm text-muted-foreground">
                {t("walletLabel", { name: transaction.wallet_name })}
              </div>
            )}
          </div>

          {isExpanded && (
            <TransactionDetails
              transaction={transaction}
              isExpanded={isExpanded}
              notifications={notifications}
              isLoadingNotifications={isLoadingNotifications}
              notificationError={notificationError}
            />
          )}
        </div>
      </CardContent>
    </Card>
  )
}
