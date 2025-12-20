"use client"

import Image from "next/image"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import { Plus, Settings } from "lucide-react"
import { UserDropdown } from "@/components/user-dropdown"
import { useAuth } from "@/contexts/auth-context"

interface AppHeaderProps {
  showAddWallet?: boolean
  customLogo?: string
}

export function AppHeader({ showAddWallet = false, customLogo }: AppHeaderProps) {
  const { isCloudMode } = useAuth()

  return (
    <div className="mb-4 sm:mb-6 flex items-center justify-between">
      <Link href="/" className="flex items-center gap-2 sm:gap-4 hover:opacity-80 transition-opacity">
        <div className="relative w-10 h-10 sm:w-12 sm:h-12">
          {customLogo ? (
            <div
              className="absolute inset-0 w-full h-full [&>svg]:w-full [&>svg]:h-full [&>svg]:transition-all [&>svg]:duration-500 [&>svg]:ease-in-out animate-in fade-in-0 duration-500"
              dangerouslySetInnerHTML={{ __html: customLogo }}
            />
          ) : (
            <Image
              src="/images/canary.svg"
              alt="Canary Logo"
              width={48}
              height={48}
              className="absolute inset-0 h-10 w-10 sm:h-12 sm:w-12 transition-all duration-500 ease-in-out animate-in fade-in-0 duration-500"
            />
          )}
        </div>
        <h1 className="text-2xl sm:text-3xl font-bold tracking-wide">Canary</h1>
      </Link>
      <div className="flex items-center gap-2 sm:gap-6">
        {showAddWallet && (
          <Link href="/wallets/add">
            <Button
              size="sm"
              className="bg-accent hover:bg-accent/90 text-accent-foreground gap-1.5 sm:gap-2"
            >
              <Plus size={16} />
              <span className="hidden sm:inline">Add Wallet</span>
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
            >
              <Settings size={16} />
              <span className="hidden sm:inline">Settings</span>
            </Button>
          </Link>
        )}

        <UserDropdown />
      </div>
    </div>
  )
}