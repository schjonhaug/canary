"use client"

import Image from "next/image"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { Button } from "@/components/ui/button"
import { Plus, Settings } from "lucide-react"
import { UserDropdown } from "@/components/user-dropdown"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

export function AppHeader() {
  const { isCloudMode, user } = useAuth()
  const pathname = usePathname()
  const tNav = useTranslations('nav')

  // Show Add Wallet button on most pages, except:
  // - On the add wallet page itself
  // - For admin users in cloud mode
  // - For demo users
  // - For logged-out users in cloud mode
  const isAddWalletPage = pathname.startsWith('/wallets/add')
  const isLoggedOut = isCloudMode && !user
  const showAddWallet = !isAddWalletPage && !isLoggedOut && !(isCloudMode && user?.is_admin) && !user?.is_demo

  return (
    <div className="mb-4 sm:mb-6 flex items-center justify-between">
      <Link href="/" className="flex items-center gap-2 sm:gap-4 hover:opacity-80 transition-opacity">
        <div className="relative w-10 h-10 sm:w-12 sm:h-12">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={48}
            height={48}
            className="absolute inset-0 h-10 w-10 sm:h-12 sm:w-12"
          />
        </div>
        <h1 className="text-2xl sm:text-3xl font-bold tracking-wide">Canary</h1>
      </Link>
      <div className="flex items-center gap-2 sm:gap-6">
        {showAddWallet && (
          <Link href="/wallets/add">
            <Button
              size="sm"
              className="bg-accent hover:bg-accent/90 text-accent-foreground gap-1.5 sm:gap-2"
              aria-label={tNav('addWallet')}
            >
              <Plus size={16} />
              <span className="hidden sm:inline" aria-hidden="true">{tNav('addWallet')}</span>
            </Button>
          </Link>
        )}

        {/* Settings button for self-hosted mode */}
        {!isCloudMode && (
          <Link href="/settings">
            <Button
              variant="outline"
              size="sm"
              className="gap-1.5 sm:gap-2"
              aria-label={tNav('settings')}
            >
              <Settings size={16} />
              <span className="hidden sm:inline" aria-hidden="true">{tNav('settings')}</span>
            </Button>
          </Link>
        )}

        <UserDropdown />
      </div>
    </div>
  )
}