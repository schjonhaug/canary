"use client"

import Image from "next/image"
import { useState } from "react"
import type { ReactNode } from "react"
import { Bell, ChevronDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import {
  NtfyServerSettingsContent,
  type NtfyServerSettingsProps,
} from "@/components/settings/ntfy-server-settings"
import { NostrSettingsContent } from "@/components/settings/nostr-settings"

export function NotificationMethodSettings(props: NtfyServerSettingsProps) {
  const t = useTranslations("settings")
  const [openProvider, setOpenProvider] = useState<"ntfy" | "nostr" | null>(null)

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Bell className="h-5 w-5" />
          {t("notifications.title")}
        </CardTitle>
        <CardDescription>{t("notifications.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <NotificationProviderPanel
          isOpen={openProvider === "ntfy"}
          onOpenChange={(isOpen) => setOpenProvider(isOpen ? "ntfy" : null)}
          icon={
            <Image
              src="/images/notifications/ntfy.svg"
              alt="ntfy logo"
              width={32}
              height={32}
              className="h-full w-full object-contain"
            />
          }
          title="ntfy"
          description={t("ntfy.description")}
        >
          <NtfyServerSettingsContent {...props} showEndpointProviderFrame={false} />
        </NotificationProviderPanel>

        <NotificationProviderPanel
          isOpen={openProvider === "nostr"}
          onOpenChange={(isOpen) => setOpenProvider(isOpen ? "nostr" : null)}
          icon={
            <Image
              src="/images/notifications/nostr.svg"
              alt="Nostr logo"
              width={32}
              height={32}
              className="h-full w-full object-contain"
            />
          }
          title={t("nostr.title")}
          description={t("nostr.description")}
        >
          <NostrSettingsContent />
        </NotificationProviderPanel>
      </CardContent>
    </Card>
  )
}

function NotificationProviderPanel({
  isOpen,
  onOpenChange,
  icon,
  title,
  description,
  children,
}: {
  isOpen: boolean
  onOpenChange: (isOpen: boolean) => void
  icon: ReactNode
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <Collapsible open={isOpen} onOpenChange={onOpenChange}>
      <div className="rounded-md border p-3">
        <div className="flex items-start gap-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-background">
            {icon}
          </div>
          <div className="min-w-0 flex-1">
            <CollapsibleTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                className="h-auto w-full justify-between gap-3 p-0 text-left text-foreground hover:bg-transparent hover:text-foreground"
              >
                <span className="min-w-0">
                  <span className="block text-sm font-medium leading-none">{title}</span>
                  <span className="mt-1 block text-sm font-normal text-muted-foreground">{description}</span>
                </span>
                <ChevronDown
                  className={`h-4 w-4 shrink-0 text-muted-foreground transition-transform ${
                    isOpen ? "rotate-180" : ""
                  }`}
                  aria-hidden="true"
                />
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="pt-4">
              {children}
            </CollapsibleContent>
          </div>
        </div>
      </div>
    </Collapsible>
  )
}
