"use client"

import { Card, CardContent } from "@/components/ui/card"
import { WalletDetailsSection } from "@/components/wallet-detail/wallet-details-section"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import type { Wallet } from "@/types"

interface WalletInfoSidebarProps {
  wallet: Wallet
  onDeleteClick: () => void
  showActions: boolean
}

export function WalletInfoSidebar({
  wallet,
  onDeleteClick,
  showActions,
}: WalletInfoSidebarProps) {
  const t = useTranslations("wallets")
  const { formatBitcoinAmount, formatFiatAmount } = useFormatters()

  return (
    <div className="lg:col-span-1 space-y-4">
      <Card>
        <CardContent className="space-y-6">
          {/* Balance Section */}
          <div>
            <div className="text-sm font-medium text-muted-foreground mb-2">
              {t("detail.balance")}
            </div>
            <div className="text-2xl font-bold font-mono">
              {formatBitcoinAmount(wallet.balance_total || 0)}
            </div>
            {wallet.balance_fiat !== undefined && wallet.fiat_currency && (
              <div className="text-sm text-muted-foreground mt-1">
                {formatFiatAmount(wallet.balance_fiat, wallet.fiat_currency)}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Wallet Details */}
      <Card>
        <CardContent>
          <WalletDetailsSection
            wallet={wallet}
            onDeleteClick={showActions ? onDeleteClick : undefined}
          />
        </CardContent>
      </Card>
    </div>
  )
}
