"use client"

import type { ComponentProps } from "react"
import { Check } from "lucide-react"
import { useTranslations } from "next-intl"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"

type SaveButtonProps = ComponentProps<typeof Button> & {
  saving?: boolean
  saved?: boolean
  idleLabel?: string
  savingLabel?: string
  savedLabel?: string
}

export function SaveButton({
  saving = false,
  saved = false,
  idleLabel,
  savingLabel,
  savedLabel,
  disabled,
  className,
  variant = "outline",
  ...props
}: SaveButtonProps) {
  const tCommon = useTranslations("common")
  const showSaved = saved && !saving

  return (
    <Button
      type="button"
      variant={variant}
      disabled={disabled || saving || saved}
      className={cn(
        showSaved && "border-green-600/40 text-green-700 disabled:opacity-100 dark:text-green-500",
        className
      )}
      {...props}
    >
      {saving ? (
        savingLabel ?? tCommon("saving")
      ) : showSaved ? (
        <>
          <Check className="canary-save-check h-4 w-4 text-green-600" aria-hidden="true" />
          <span aria-live="polite">{savedLabel ?? tCommon("saved")}</span>
        </>
      ) : (
        idleLabel ?? tCommon("save")
      )}
    </Button>
  )
}
