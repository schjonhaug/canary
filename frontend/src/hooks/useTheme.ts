"use client"

import { useEffect, useState } from "react"
import {
  applyTheme,
  isThemePreference,
  resolveTheme,
  THEME_STORAGE_KEY,
  type ResolvedTheme,
  type ThemePreference,
} from "@/lib/theme"

interface UseThemeResult {
  preference: ThemePreference
  resolvedTheme: ResolvedTheme
  setPreference: (nextPreference: ThemePreference) => void
}

function getStoredThemePreference(): ThemePreference {
  if (typeof window === "undefined") {
    return "system"
  }

  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY)
    return isThemePreference(stored) ? stored : "system"
  } catch {
    return "system"
  }
}

function getSystemPrefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches
}

export function useTheme(): UseThemeResult {
  const [preference, setPreferenceState] = useState<ThemePreference>("system")
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>("light")

  useEffect(() => {
    const nextPreference = getStoredThemePreference()
    const nextResolvedTheme = resolveTheme(nextPreference, getSystemPrefersDark())

    setPreferenceState(nextPreference)
    setResolvedTheme(nextResolvedTheme)
    applyTheme(nextResolvedTheme)

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)")

    const handleSystemThemeChange = (event: MediaQueryListEvent) => {
      setResolvedTheme((currentResolvedTheme) => {
        if (getStoredThemePreference() !== "system") {
          return currentResolvedTheme
        }

        const nextTheme = event.matches ? "dark" : "light"
        applyTheme(nextTheme)
        return nextTheme
      })
    }

    mediaQuery.addEventListener("change", handleSystemThemeChange)

    return () => {
      mediaQuery.removeEventListener("change", handleSystemThemeChange)
    }
  }, [])

  const setPreference = (nextPreference: ThemePreference) => {
    const nextResolvedTheme = resolveTheme(nextPreference, getSystemPrefersDark())

    setPreferenceState(nextPreference)
    setResolvedTheme(nextResolvedTheme)
    applyTheme(nextResolvedTheme)

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, nextPreference)
    } catch {}
  }

  return { preference, resolvedTheme, setPreference }
}
