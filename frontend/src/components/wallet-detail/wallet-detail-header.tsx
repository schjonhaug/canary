"use client"

import Link from "next/link"
import { InlineWalletNameEdit } from "@/components/inline-wallet-name-edit"
import { useTranslations } from "next-intl"

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

  return (
    <section>
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
    </section>
  )
}
