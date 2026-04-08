"use client"

import { createContext, createElement, useContext, useEffect, useState, type ReactNode } from "react"
import {
  applyTheme,
  isThemePreference,
  resolveTheme,
  THEME_STORAGE_KEY,
  type ResolvedTheme,
  type ThemePreference,
} from "@/lib/theme"

interface UseThemeResult {
  mounted: boolean
  preference: ThemePreference
  resolvedTheme: ResolvedTheme
  setPreference: (nextPreference: ThemePreference) => void
}

const ThemeContext = createContext<UseThemeResult | null>(null)

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

function useThemeState(): UseThemeResult {
  const [mounted, setMounted] = useState(false)
  const [preference, setPreferenceState] = useState<ThemePreference>(() => getStoredThemePreference())
  const [systemPrefersDark, setSystemPrefersDark] = useState<boolean>(() => getSystemPrefersDark())
  const resolvedTheme = resolveTheme(preference, systemPrefersDark)

  useEffect(() => {
    setMounted(true)
  }, [])

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)")

    const handleSystemThemeChange = (event: MediaQueryListEvent) => {
      setSystemPrefersDark(event.matches)
    }

    setSystemPrefersDark(mediaQuery.matches)

    if (typeof mediaQuery.addEventListener === "function") {
      mediaQuery.addEventListener("change", handleSystemThemeChange)

      return () => {
        mediaQuery.removeEventListener("change", handleSystemThemeChange)
      }
    }

    mediaQuery.addListener(handleSystemThemeChange)

    return () => {
      mediaQuery.removeListener(handleSystemThemeChange)
    }
  }, [])

  useEffect(() => {
    const handleStorage = (event: StorageEvent) => {
      if (event.key !== THEME_STORAGE_KEY) {
        return
      }

      setPreferenceState(isThemePreference(event.newValue) ? event.newValue : "system")
    }

    window.addEventListener("storage", handleStorage)

    return () => {
      window.removeEventListener("storage", handleStorage)
    }
  }, [])

  useEffect(() => {
    applyTheme(resolvedTheme)
  }, [resolvedTheme])

  const setPreference = (nextPreference: ThemePreference) => {
    setPreferenceState(nextPreference)

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, nextPreference)
    } catch {}
  }

  return { mounted, preference, resolvedTheme, setPreference }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const value = useThemeState()

  return createElement(ThemeContext.Provider, { value }, children)
}

export function useTheme(): UseThemeResult {
  const context = useContext(ThemeContext)

  if (!context) {
    throw new Error("useTheme must be used within ThemeProvider")
  }

  return context
}
