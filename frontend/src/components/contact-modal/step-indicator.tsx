"use client"

import { cn } from "@/lib/utils"

interface StepIndicatorProps {
  currentStep: number
  totalSteps: number
}

export function StepIndicator({ currentStep, totalSteps }: StepIndicatorProps) {
  return (
    <div className="flex items-center justify-center gap-1.5">
      {Array.from({ length: totalSteps }, (_, i) => (
        <div
          key={i}
          className={cn(
            "h-1.5 rounded-full transition-all duration-200",
            i === currentStep
              ? "w-6 bg-primary"
              : i < currentStep
                ? "w-1.5 bg-primary/50"
                : "w-1.5 bg-muted-foreground/25"
          )}
        />
      ))}
    </div>
  )
}
