"use client"

import { Monitor, Moon, Sun } from "lucide-react"
import { useTranslations } from "next-intl"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useTheme } from "@/hooks/useTheme"
import { isThemePreference, type ThemePreference } from "@/lib/theme"

const themeIcons = {
  system: Monitor,
  light: Sun,
  dark: Moon,
} satisfies Record<ThemePreference, typeof Monitor>

export function ThemeSettings() {
  const t = useTranslations("settings")
  const { mounted, preference, resolvedTheme, setPreference } = useTheme()

  const handleThemeChange = (value: string) => {
    if (isThemePreference(value)) {
      setPreference(value)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Moon className="h-5 w-5" />
          {t("appearance.title")}
        </CardTitle>
        <CardDescription>{t("appearance.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div>
          <Label htmlFor="theme">{t("appearance.themeLabel")}</Label>
          <Select value={preference} onValueChange={handleThemeChange}>
            <SelectTrigger id="theme" className="w-full max-w-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(["system", "light", "dark"] as const).map((value) => {
                const Icon = themeIcons[value]

                return (
                  <SelectItem key={value} value={value}>
                    <span className="flex items-center gap-2">
                      <Icon className="h-4 w-4" />
                      <span>{t(`appearance.options.${value}`)}</span>
                    </span>
                  </SelectItem>
                )
              })}
            </SelectContent>
          </Select>
        </div>

        {mounted ? (
          <p className="text-sm text-muted-foreground">
            {t(`appearance.current.${resolvedTheme}`)}
          </p>
        ) : (
          <div aria-hidden="true" className="h-5" />
        )}
      </CardContent>
    </Card>
  )
}
