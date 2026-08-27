"use client"

import { ChevronDown } from "lucide-react"
import { useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type { ContactDraft, TransactionDraftKey } from "./types"

const DIRECTIONAL_ALERTS: Array<{
  key: TransactionDraftKey
  labelKey: string
  descriptionKey: string
}> = [
  { key: "notify_sending", labelKey: "events.sending.label", descriptionKey: "events.sending.description" },
  { key: "notify_receiving", labelKey: "events.receiving.label", descriptionKey: "events.receiving.description" },
  { key: "notify_sent", labelKey: "events.sent.label", descriptionKey: "events.sent.description" },
  { key: "notify_received", labelKey: "events.received.label", descriptionKey: "events.received.description" },
]

function groupState(first: boolean, second: boolean): boolean | "indeterminate" {
  return first === second ? first : "indeterminate"
}

function AlertCheckbox({
  checked,
  label,
  description,
  onChange,
  disabled,
}: {
  checked: boolean | "indeterminate"
  label: string
  description: string
  onChange: (checked: boolean) => void
  disabled?: boolean
}) {
  return (
    <label className="flex items-start gap-3 rounded-md border p-3 text-sm">
      <Checkbox
        checked={checked}
        disabled={disabled}
        onCheckedChange={(value) => onChange(value === true)}
        aria-label={label}
      />
      <span className="space-y-1">
        <span className="block font-medium leading-none">{label}</span>
        <span className="block text-xs leading-snug text-muted-foreground">{description}</span>
      </span>
    </label>
  )
}

export function AlertTimingControls({
  draft,
  onChange,
  disabled = false,
}: {
  draft: ContactDraft
  onChange: (draft: ContactDraft) => void
  disabled?: boolean
}) {
  const t = useTranslations("walletNotifications")
  const [advancedOpen, setAdvancedOpen] = useState(false)

  const setFields = (values: Partial<ContactDraft>) => onChange({ ...draft, ...values })

  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-2">
        <AlertCheckbox
          checked={groupState(draft.notify_sending, draft.notify_receiving)}
          label={t("timing.activity.label")}
          description={t("timing.activity.description")}
          disabled={disabled}
          onChange={(checked) => setFields({ notify_sending: checked, notify_receiving: checked })}
        />
        <AlertCheckbox
          checked={groupState(draft.notify_sent, draft.notify_received)}
          label={t("timing.confirmation.label")}
          description={t("timing.confirmation.description")}
          disabled={disabled}
          onChange={(checked) => setFields({ notify_sent: checked, notify_received: checked })}
        />
      </div>

      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen} className="rounded-md border">
        <CollapsibleTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-between p-3 text-left font-medium"
          >
            <span>
              {t("timing.advanced.title")}
              <span className="mt-1 block text-xs font-normal text-muted-foreground">
                {t("timing.advanced.description")}
              </span>
            </span>
            <ChevronDown
              className={`h-4 w-4 transition-transform ${advancedOpen ? "rotate-180" : ""}`}
              aria-hidden="true"
            />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="grid gap-4 border-t p-3 md:grid-cols-2">
            <div className="space-y-3">
              <h4 className="text-xs font-semibold uppercase text-muted-foreground">
                {t("timing.advanced.directional")}
              </h4>
              {DIRECTIONAL_ALERTS.map(({ key, labelKey, descriptionKey }) => (
                <AlertCheckbox
                  key={key}
                  checked={draft[key]}
                  label={t(labelKey)}
                  description={t(descriptionKey)}
                  disabled={disabled}
                  onChange={(checked) => setFields({ [key]: checked })}
                />
              ))}
            </div>
            <div className="space-y-3">
              <h4 className="text-xs font-semibold uppercase text-muted-foreground">
                {t("timing.advanced.feeManagement")}
              </h4>
              {(["notify_rbf", "notify_cpfp"] as const).map((key) => (
                <AlertCheckbox
                  key={key}
                  checked={draft[key]}
                  label={t(key === "notify_rbf" ? "events.rbf.label" : "events.cpfp.label")}
                  description={t(key === "notify_rbf" ? "events.rbf.description" : "events.cpfp.description")}
                  disabled={disabled}
                  onChange={(checked) => setFields({ [key]: checked })}
                />
              ))}
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}
