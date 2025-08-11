"use client"

import { Switch } from "@/components/ui/switch"

interface BillingToggleProps {
  isYearly: boolean
  onToggle: (isYearly: boolean) => void
  className?: string
}

export function BillingToggle({ isYearly, onToggle, className = "" }: BillingToggleProps) {
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
        {isYearly && (
          <span className="ml-1.5 text-xs bg-green-100 text-green-700 px-1.5 py-0.5 rounded-full">
            Save 20%
          </span>
        )}
      </span>
    </div>
  )
}