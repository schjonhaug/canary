"use client"

import Image from "next/image"
import Link from "next/link"
import { Button } from "@/components/ui/button"
import { Plus } from "lucide-react"
import { UserDropdown } from "@/components/user-dropdown"

interface AppHeaderProps {
  showCreateWallet?: boolean
  onCreateWallet?: () => void
  customLogo?: string
}

export function AppHeader({ showCreateWallet = false, onCreateWallet, customLogo }: AppHeaderProps) {
  return (
    <div className="mb-6 flex items-center justify-between">
      <Link href="/" className="flex items-center gap-4 hover:opacity-80 transition-opacity">
        <div className="relative w-12 h-12">
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
              className="absolute inset-0 h-12 w-12 transition-all duration-500 ease-in-out animate-in fade-in-0 duration-500"
            />
          )}
        </div>
        <h1 className="text-3xl font-bold tracking-wide">Canary</h1>
      </Link>
      <div className="flex items-center gap-6">
        {showCreateWallet && onCreateWallet && (
          <Button
            onClick={onCreateWallet}
            size="sm"
            className="bg-accent hover:bg-accent/90 text-accent-foreground gap-2"
          >
            <Plus size={16} />
            Create Wallet
          </Button>
        )}
        
        <UserDropdown />
      </div>
    </div>
  )
}