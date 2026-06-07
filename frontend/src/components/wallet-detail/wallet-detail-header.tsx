"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { InlineWalletNameEdit } from "@/components/inline-wallet-name-edit"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"

interface WalletDetailHeaderProps {
  walletChecksum: string
  walletName: string
  onNameUpdated: (newName: string) => void
}

export function WalletDetailHeader({
  walletChecksum,
  walletName,
  onNameUpdated,
}: WalletDetailHeaderProps) {
  const t = useTranslations("wallets")
  const pathname = usePathname()
  const tabs = [
    {
      href: `/wallets/${walletChecksum}/transactions`,
      label: t("detail.transactions"),
      active: pathname.endsWith("/transactions"),
    },
    {
      href: `/wallets/${walletChecksum}/notifications`,
      label: t("detail.notifications"),
      active: pathname.endsWith("/notifications"),
    },
  ]

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3 flex-wrap min-w-0">
          <div className="min-w-0">
            <nav className="flex items-center text-2xl text-muted-foreground min-w-0">
              <Link
                href="/wallets"
                className="hover:text-foreground font-semibold flex-shrink-0"
              >
                {t("title")}
              </Link>
              <span className="mx-2 flex-shrink-0">/</span>
              <div className="text-foreground font-semibold min-w-0">
                <InlineWalletNameEdit
                  walletChecksum={walletChecksum}
                  currentName={walletName}
                  onNameUpdated={onNameUpdated}
                />
              </div>
            </nav>
          </div>
        </div>
      </div>
      <nav className="flex border-b" aria-label={t("detail.walletSections")}>
        {tabs.map((tab) => (
          <Link
            key={tab.href}
            href={tab.href}
            className={cn(
              "-mb-px border-b-2 px-4 py-2 text-sm font-medium transition-colors",
              tab.active
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground"
            )}
            aria-current={tab.active ? "page" : undefined}
          >
            {tab.label}
          </Link>
        ))}
      </nav>
    </section>
  )
}
