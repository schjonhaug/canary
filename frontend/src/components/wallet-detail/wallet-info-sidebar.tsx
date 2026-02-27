"use client"

import { Plus, Trash2 } from "lucide-react"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { WalletContactsList } from "@/components/wallet-contacts-list"
import { BalanceAlertsList } from "@/components/balance-alerts-list"
import { WalletDetailsSection } from "@/components/wallet-detail/wallet-details-section"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import type { Wallet, Contact, BalanceAlert } from "@/types"

interface WalletInfoSidebarProps {
  wallet: Wallet
  contacts: Contact[]
  balanceAlerts: BalanceAlert[]
  onAddContact: () => void
  onContactsUpdated: () => void
  onDeleteClick: () => void
  showActions: boolean
}

export function WalletInfoSidebar({
  wallet,
  contacts,
  balanceAlerts,
  onAddContact,
  onContactsUpdated,
  onDeleteClick,
  showActions,
}: WalletInfoSidebarProps) {
  const t = useTranslations("wallets")
  const tCommon = useTranslations("common")
  const { formatBitcoinAmount, formatFiatAmount } = useFormatters()

  return (
    <div className="lg:col-span-1">
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

          {/* Contacts Section */}
          <div className="pt-2 border-t">
            <div className="flex items-center justify-between mb-2">
              <div className="text-sm font-medium text-muted-foreground">
                {t("detail.contacts")}
              </div>
              {showActions && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={onAddContact}
                  className="h-6 px-2 text-xs gap-1"
                >
                  <Plus className="h-3 w-3" />
                  {tCommon("new")}
                </Button>
              )}
            </div>
            <WalletContactsList
              walletChecksum={wallet.checksum}
              contacts={contacts}
              onContactsUpdated={onContactsUpdated}
              isWalletActive={wallet.is_active !== false}
            />
          </div>

          {/* Balance Alerts Section */}
          <div className="pt-2 border-t">
            <BalanceAlertsList
              walletChecksum={wallet.checksum}
              balanceAlerts={balanceAlerts}
            />
          </div>

          {/* Wallet Details */}
          <div className="pt-2 border-t">
            <WalletDetailsSection wallet={wallet} />
          </div>

          {/* Delete Button */}
          {showActions && (
            <div className="pt-2 border-t flex justify-end">
              <Button
                variant="ghost"
                size="sm"
                onClick={onDeleteClick}
                className="text-muted-foreground hover:text-red-600"
              >
                <Trash2 size={16} />
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
