"use client"

import { Loader2 } from "lucide-react"

import { cn } from "@/lib/utils"

type LoadingSpinnerSize = "sm" | "md" | "lg"

const sizeClasses: Record<LoadingSpinnerSize, string> = {
  sm: "h-4 w-4",
  md: "h-8 w-8",
  lg: "h-12 w-12",
}

interface LoadingSpinnerProps {
  size?: LoadingSpinnerSize
  className?: string
  "aria-label"?: string
}

export function LoadingSpinner({
  size = "md",
  className,
  "aria-label": ariaLabel = "Loading",
}: LoadingSpinnerProps) {
  return (
    <Loader2
      aria-label={ariaLabel}
      className={cn("animate-spin text-primary", sizeClasses[size], className)}
    />
  )
}

