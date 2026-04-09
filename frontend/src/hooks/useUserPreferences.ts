import { useState, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { api } from "@/lib/api"
import { getStoredLocale, setStoredLocale } from "@/lib/locale"
import type { Locale } from "@/i18n/config"

export interface UserPreferences {
  preferred_fiat_currency: string
  ntfy_server_url: string | null
  ntfy_target_options: NtfyTargetOption[]
  ntfy_has_access_token: boolean
  ntfy_has_credentials: boolean
  ntfy_username: string | null
}

export type NtfyAuthType = "none" | "token" | "basic"
export type NtfyTargetType = "public" | "umbrel" | "custom"

export interface NtfyTargetOption {
  id: string
  label: string
  url: string
}

function getTargetTypeForUrl(url: string, options: NtfyTargetOption[]): NtfyTargetType {
  if (!url || url === "https://ntfy.sh") {
    return "public"
  }
  if (options.some((option) => option.id === "umbrel" && option.url === url)) {
    return "umbrel"
  }
  return "custom"
}

function getUrlForTargetType(targetType: NtfyTargetType, customUrl: string, options: NtfyTargetOption[]): string {
  if (targetType === "public") {
    return ""
  }
  if (targetType === "umbrel") {
    return options.find((option) => option.id === "umbrel")?.url ?? customUrl
  }
  return customUrl
}

interface UseUserPreferencesOptions {
  isAuthenticated: boolean
}

export function useUserPreferences({ isAuthenticated }: UseUserPreferencesOptions) {
  const router = useRouter()

  // Regional settings state
  const [selectedCurrency, setSelectedCurrency] = useState<string>("USD")
  const [currentLocale, setCurrentLocale] = useState<Locale>("en-US")
  const [isUpdatingCurrency, setIsUpdatingCurrency] = useState(false)

  // User preferences from API
  const [userPreferences, setUserPreferences] = useState<UserPreferences | null>(null)

  // ntfy server state
  const [ntfyServerUrl, setNtfyServerUrl] = useState<string>("")
  const [savedNtfyUrl, setSavedNtfyUrl] = useState<string>("")
  const [ntfyTargetType, setNtfyTargetType] = useState<NtfyTargetType>("public")
  const [savedNtfyTargetType, setSavedNtfyTargetType] = useState<NtfyTargetType>("public")
  const [customNtfyServerUrl, setCustomNtfyServerUrl] = useState<string>("")

  // ntfy authentication state
  const [ntfyAuthType, setNtfyAuthType] = useState<NtfyAuthType>("none")
  const [savedNtfyAuthType, setSavedNtfyAuthType] = useState<NtfyAuthType>("none")
  const [ntfyAccessToken, setNtfyAccessToken] = useState<string>("")
  const [ntfyUsername, setNtfyUsername] = useState<string>("")
  const [savedNtfyUsername, setSavedNtfyUsername] = useState<string>("")
  const [ntfyPassword, setNtfyPassword] = useState<string>("")

  // Consolidated ntfy save state
  const [isUpdatingNtfySettings, setIsUpdatingNtfySettings] = useState(false)
  const [ntfySettingsError, setNtfySettingsError] = useState<string | null>(null)
  const [ntfySettingsSuccess, setNtfySettingsSuccess] = useState(false)

  const hasSelectableNtfyTargets = Boolean(userPreferences?.ntfy_target_options.some((option) => option.id === "umbrel"))
  const currentNtfyServerUrl = hasSelectableNtfyTargets
    ? getUrlForTargetType(ntfyTargetType, customNtfyServerUrl, userPreferences?.ntfy_target_options ?? [])
    : ntfyServerUrl

  // Derived state - any ntfy field has changed
  const hasAnyNtfyChanges =
    currentNtfyServerUrl !== savedNtfyUrl ||
    (hasSelectableNtfyTargets && ntfyTargetType !== savedNtfyTargetType) ||
    ntfyAuthType !== savedNtfyAuthType ||
    (ntfyAuthType === "token" && ntfyAccessToken.trim() !== "") ||
    (ntfyAuthType === "basic" && ntfyPassword.trim() !== "") ||
    (ntfyAuthType === "basic" && ntfyUsername !== savedNtfyUsername)

  // Auto-clear success message after 3 seconds
  useEffect(() => {
    if (ntfySettingsSuccess) {
      const timerId = setTimeout(() => setNtfySettingsSuccess(false), 3000)
      return () => clearTimeout(timerId)
    }
  }, [ntfySettingsSuccess])

  // Fetch user preferences on mount
  useEffect(() => {
    const fetchPreferences = async () => {
      try {
        const prefs = await api.getUserPreferences()
        setUserPreferences(prefs)
        setSelectedCurrency(prefs.preferred_fiat_currency)
        setNtfyServerUrl(prefs.ntfy_server_url || "")
        setSavedNtfyUrl(prefs.ntfy_server_url || "")
        const targetType = getTargetTypeForUrl(prefs.ntfy_server_url || "", prefs.ntfy_target_options)
        setNtfyTargetType(targetType)
        setSavedNtfyTargetType(targetType)
        setCustomNtfyServerUrl(targetType === "custom" ? prefs.ntfy_server_url || "" : "")

        // Set auth type based on what's configured
        if (prefs.ntfy_has_access_token) {
          setNtfyAuthType("token")
          setSavedNtfyAuthType("token")
        } else if (prefs.ntfy_has_credentials) {
          setNtfyAuthType("basic")
          setSavedNtfyAuthType("basic")
          setNtfyUsername(prefs.ntfy_username || "")
          setSavedNtfyUsername(prefs.ntfy_username || "")
        } else {
          setNtfyAuthType("none")
          setSavedNtfyAuthType("none")
        }
      } catch (error) {
        console.error("Failed to fetch user preferences:", error)
        setSelectedCurrency("USD")
      }
    }

    if (isAuthenticated) {
      fetchPreferences()
    }
  }, [isAuthenticated])

  // Initialize locale from cookie
  useEffect(() => {
    setCurrentLocale(getStoredLocale())
  }, [])

  const handleLanguageChange = useCallback(
    async (locale: Locale) => {
      setStoredLocale(locale)
      setCurrentLocale(locale)

      // Sync to backend for authenticated users
      if (isAuthenticated) {
        try {
          await api.updateUserPreferences({ preferred_language: locale })
        } catch (error) {
          console.error("Failed to sync language preference:", error)
        }
      }

      // Refresh to apply new locale
      router.refresh()
    },
    [isAuthenticated, router]
  )

  const handleCurrencyChange = useCallback(
    async (currency: string) => {
      setSelectedCurrency(currency)
      setIsUpdatingCurrency(true)

      try {
        const result = await api.updateUserPreferences({ preferred_fiat_currency: currency })
        setUserPreferences(result)
      } catch (error) {
        console.error("Failed to update currency preference:", error)
        // Revert on error
        if (userPreferences) {
          setSelectedCurrency(userPreferences.preferred_fiat_currency)
        }
      } finally {
        setIsUpdatingCurrency(false)
      }
    },
    [userPreferences]
  )

  const handleNtfySettingsSave = useCallback(async () => {
    setIsUpdatingNtfySettings(true)
    setNtfySettingsError(null)
    setNtfySettingsSuccess(false)

    // Only validate/include auth fields when auth settings actually changed
    const authChanged =
      ntfyAuthType !== savedNtfyAuthType ||
      (ntfyAuthType === "token" && ntfyAccessToken.trim() !== "") ||
      (ntfyAuthType === "basic" && (ntfyPassword.trim() !== "" || ntfyUsername !== savedNtfyUsername))

    try {
      let updateData: {
        ntfy_server_url?: string
        ntfy_access_token?: string
        ntfy_username?: string
        ntfy_password?: string
      } = {
        ntfy_server_url: currentNtfyServerUrl,
      }

      if (authChanged) {
        if (ntfyAuthType === "none") {
          updateData = {
            ...updateData,
            ntfy_access_token: "",
            ntfy_username: "",
            ntfy_password: "",
          }
        } else if (ntfyAuthType === "token") {
          if (!ntfyAccessToken.trim()) {
            setNtfySettingsError("Access token is required")
            return
          }
          updateData = { ...updateData, ntfy_access_token: ntfyAccessToken.trim() }
        } else if (ntfyAuthType === "basic") {
          if (!ntfyUsername.trim() || !ntfyPassword.trim()) {
            setNtfySettingsError("Both username and password are required")
            return
          }
          updateData = {
            ...updateData,
            ntfy_username: ntfyUsername.trim(),
            ntfy_password: ntfyPassword.trim(),
          }
        }
      }

      const result = await api.updateUserPreferences(updateData)
      const savedTargetType = getTargetTypeForUrl(result.ntfy_server_url || "", result.ntfy_target_options)
      setUserPreferences(result)
      setSavedNtfyUrl(result.ntfy_server_url || "")
      setNtfyServerUrl(result.ntfy_server_url || "")
      setNtfyTargetType(savedTargetType)
      setSavedNtfyTargetType(savedTargetType)
      setCustomNtfyServerUrl(savedTargetType === "custom" ? result.ntfy_server_url || "" : "")
      if (authChanged) {
        setSavedNtfyAuthType(ntfyAuthType)
        if (ntfyAuthType === "basic") {
          setSavedNtfyUsername(ntfyUsername.trim())
        }
      }

      // Clear sensitive fields after save
      setNtfyAccessToken("")
      setNtfyPassword("")

      setNtfySettingsSuccess(true)
    } catch (error) {
      console.error("Failed to update ntfy settings:", error)
      setNtfySettingsError(error instanceof Error ? error.message : "Failed to save")
      // Revert all fields to saved state
      setNtfyServerUrl(savedNtfyUrl)
      setNtfyTargetType(savedNtfyTargetType)
      setCustomNtfyServerUrl(savedNtfyTargetType === "custom" ? savedNtfyUrl : "")
      setNtfyAuthType(savedNtfyAuthType)
      if (savedNtfyAuthType === "basic") {
        setNtfyUsername(savedNtfyUsername)
      }
    } finally {
      setIsUpdatingNtfySettings(false)
    }
  }, [currentNtfyServerUrl, savedNtfyUrl, savedNtfyTargetType, ntfyAuthType, savedNtfyAuthType, ntfyAccessToken, ntfyUsername, savedNtfyUsername, ntfyPassword])

  const clearNtfySettingsErrors = useCallback(() => {
    setNtfySettingsError(null)
    setNtfySettingsSuccess(false)
  }, [])

  return {
    // Regional settings
    currentLocale,
    selectedCurrency,
    isUpdatingCurrency,
    handleLanguageChange,
    handleCurrencyChange,

    // ntfy settings (consolidated)
    ntfyServerUrl,
    setNtfyServerUrl,
    ntfyTargetType,
    setNtfyTargetType,
    customNtfyServerUrl,
    setCustomNtfyServerUrl,
    hasAnyNtfyChanges,
    isUpdatingNtfySettings,
    ntfySettingsError,
    ntfySettingsSuccess,
    handleNtfySettingsSave,
    clearNtfySettingsErrors,

    // ntfy auth
    userPreferences,
    ntfyAuthType,
    setNtfyAuthType,
    ntfyAccessToken,
    setNtfyAccessToken,
    ntfyUsername,
    setNtfyUsername,
    ntfyPassword,
    setNtfyPassword,
  }
}
