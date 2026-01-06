'use client'

import { useState } from 'react'
import { useAuth } from '@/contexts/auth-context'
import { useRouter } from 'next/navigation'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { User, LogOut, ChevronDown, CreditCard, Settings, MessageSquare } from 'lucide-react'
import { getTierDisplayName } from '@/lib/pricing-data'
import Link from 'next/link'
import { useTranslations } from 'next-intl'

export function UserDropdown() {
  const { user, isCloudMode, isSelfHostedMode } = useAuth()
  const router = useRouter()
  const [isOpen, setIsOpen] = useState(false)
  const tNav = useTranslations('nav')

  // In self-hosted mode or if no user, don't show anything
  if (isSelfHostedMode || !user) {
    return null
  }

  const displayName = user.name || user.email
  const currentTier = user.subscription_tier || 'personal'
  const isDemoUser = user.email === 'demo@canarybitcoin.com'

  return (
    <DropdownMenu open={isOpen} onOpenChange={setIsOpen}>
      <DropdownMenuTrigger asChild>
        <Button 
          variant="ghost" 
          className="flex items-center gap-2 px-3"
        >
          <User className="h-4 w-4" />
          <span className="max-w-[150px] truncate">{displayName}</span>
          <ChevronDown className={`h-4 w-4 transition-transform ${isOpen ? 'rotate-180' : ''}`} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <div className="flex items-start justify-between gap-2 p-3">
          <div className="flex flex-col space-y-1 leading-none min-w-0 flex-1">
            {user.name && (
              <p className="text-sm font-medium truncate">{user.name}</p>
            )}
            {!isDemoUser && (
              <p className="text-xs text-muted-foreground truncate">{user.email}</p>
            )}
            {!user.is_admin && !isDemoUser && (
              <div className="flex items-center gap-2 mt-1">
                <Badge variant="secondary" className="text-xs">
                  {getTierDisplayName(currentTier)}
                </Badge>
              </div>
            )}
          </div>
        </div>
        
        {isCloudMode && !isDemoUser && (
          <>
            <DropdownMenuSeparator />

            <Link href="/settings" className="block">
              <DropdownMenuItem className="cursor-pointer">
                <Settings className="mr-2 h-4 w-4" />
                <span>{tNav('settings')}</span>
              </DropdownMenuItem>
            </Link>

            <Link href="/contact" className="block">
              <DropdownMenuItem className="cursor-pointer">
                <MessageSquare className="mr-2 h-4 w-4" />
                <span>{tNav('contact')}</span>
              </DropdownMenuItem>
            </Link>

            {!user.is_admin && (
              <Link href="/subscription" className="block">
                <DropdownMenuItem className="cursor-pointer">
                  <CreditCard className="mr-2 h-4 w-4" />
                  <span>{tNav('subscription')}</span>
                </DropdownMenuItem>
              </Link>
            )}

            <DropdownMenuSeparator />
          </>
        )}

        {isCloudMode && isDemoUser && (
          <DropdownMenuSeparator />
        )}

        <DropdownMenuItem
          className="cursor-pointer"
          onClick={() => router.push('/sign-out')}
        >
          <LogOut className="mr-2 h-4 w-4" />
          <span>{tNav('signOut')}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}