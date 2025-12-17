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

export function UserDropdown() {
  const { user, billingStatus, isCloudMode, isSelfHostedMode } = useAuth()
  const router = useRouter()
  const [isOpen, setIsOpen] = useState(false)

  // In FOSS mode or if no user, don't show anything
  if (isSelfHostedMode || !user) {
    return null
  }

  const displayName = user.name || user.email
  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
  const isDemoUser = user.email === 'demo@canarybitcoin.com'
  // const hasStripeCustomer = Boolean(billingStatus?.stripe_customer_id)

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
                <span>Settings</span>
              </DropdownMenuItem>
            </Link>

            <Link href="/contact" className="block">
              <DropdownMenuItem className="cursor-pointer">
                <MessageSquare className="mr-2 h-4 w-4" />
                <span>Contact</span>
              </DropdownMenuItem>
            </Link>

            {!user.is_admin && (
              <Link href="/settings/subscription" className="block">
                <DropdownMenuItem className="cursor-pointer">
                  <CreditCard className="mr-2 h-4 w-4" />
                  <span>Subscription</span>
                </DropdownMenuItem>
              </Link>
            )}

            {billingStatus && !user.is_admin && (
              <div className="px-2 py-1">
                <div className="text-xs text-muted-foreground space-y-1">
                  <div className="flex justify-between">
                    <span>Wallets:</span>
                    <span>{billingStatus.wallet_count} / {billingStatus.limits?.max_wallets === -1 ? '∞' : billingStatus.limits?.max_wallets}</span>
                  </div>
                  <div className="flex justify-between">
                    <span>Sync:</span>
                    <span>{billingStatus.limits?.sync_interval_seconds < 60 ? `${billingStatus.limits.sync_interval_seconds}s` : `${Math.round(billingStatus.limits.sync_interval_seconds / 60)}min`}</span>
                  </div>
                </div>
              </div>
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
          <span>Sign out</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}