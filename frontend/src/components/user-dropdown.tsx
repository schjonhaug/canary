'use client'

import { useState } from 'react'
import { useAuth } from '@/contexts/auth-context'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { User, LogOut, ChevronDown, CreditCard } from 'lucide-react'
import { getTierDisplayName } from '@/lib/pricing-data'
import Link from 'next/link'

export function UserDropdown() {
  const { user, billingStatus, logout } = useAuth()
  const [isOpen, setIsOpen] = useState(false)

  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED !== 'false'
  
  // In FOSS mode or if no user, don't show anything
  if (!authEnabled || !user) {
    return null
  }

  const displayName = user.name || user.email
  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || 'personal'
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
            <p className="text-xs text-muted-foreground truncate">{user.email}</p>
            <div className="flex items-center gap-2 mt-1">
              <Badge variant="secondary" className="text-xs">
                {getTierDisplayName(currentTier)}
              </Badge>
              {user.is_admin && (
                <Badge variant="outline" className="text-xs">
                  Admin
                </Badge>
              )}
            </div>
          </div>
        </div>
        
        <DropdownMenuSeparator />
        
        <Link href="/billing" className="block">
          <DropdownMenuItem className="cursor-pointer">
            <CreditCard className="mr-2 h-4 w-4" />
            <span>Billing & Plans</span>
          </DropdownMenuItem>
        </Link>
        
        {billingStatus && (
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
        
        <DropdownMenuItem
          className="cursor-pointer"
          onClick={() => logout()}
        >
          <LogOut className="mr-2 h-4 w-4" />
          <span>Log out</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}