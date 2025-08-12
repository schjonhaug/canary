"use client"

import { Switch } from "@/components/ui/switch"

interface BillingToggleProps {
  isYearly: boolean
  onToggle: (isYearly: boolean) => void
  discountPercent?: number
  className?: string
}

export function BillingToggle({ isYearly, onToggle, discountPercent = 20, className = "" }: BillingToggleProps) {
  return (
    <div className={`flex items-center justify-center gap-3 ${className}`}>
      <span className={`text-sm ${!isYearly ? 'font-semibold' : 'text-muted-foreground'}`}>
        Monthly
      </span>
      <Switch
        checked={isYearly}
        onCheckedChange={onToggle}
        aria-label="Toggle yearly billing"
      />
      <span className={`text-sm ${isYearly ? 'font-semibold' : 'text-muted-foreground'}`}>
        Yearly
        <span className="ml-1.5 text-xs text-muted-foreground">
          (save {Math.round(discountPercent)}%)
        </span>
      </span>
    </div>
  )
}