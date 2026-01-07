import { useState, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { api } from "@/lib/api"
import { getStoredLocale, setStoredLocale } from "@/lib/locale"
import type { Locale } from "@/i18n/config"

export interface UserPreferences {
  preferred_fiat_currency: string
  ntfy_server_url: string | null
  ntfy_has_access_token: boolean
  ntfy_has_credentials: boolean
  ntfy_username: string | null
}

export type NtfyAuthType = "none" | "token" | "basic"

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
  const [isUpdatingNtfy, setIsUpdatingNtfy] = useState(false)
  const [ntfyError, setNtfyError] = useState<string | null>(null)
  const [ntfySuccess, setNtfySuccess] = useState(false)

  // ntfy authentication state
  const [ntfyAuthType, setNtfyAuthType] = useState<NtfyAuthType>("none")
  const [ntfyAccessToken, setNtfyAccessToken] = useState<string>("")
  const [ntfyUsername, setNtfyUsername] = useState<string>("")
  const [ntfyPassword, setNtfyPassword] = useState<string>("")
  const [isUpdatingNtfyAuth, setIsUpdatingNtfyAuth] = useState(false)
  const [ntfyAuthError, setNtfyAuthError] = useState<string | null>(null)
  const [ntfyAuthSuccess, setNtfyAuthSuccess] = useState(false)

  // Derived state
  const hasNtfyChanges = ntfyServerUrl !== savedNtfyUrl

  // Fetch user preferences on mount
  useEffect(() => {
    const fetchPreferences = async () => {
      try {
        const prefs = await api.getUserPreferences()
        setUserPreferences(prefs)
        setSelectedCurrency(prefs.preferred_fiat_currency)
        setNtfyServerUrl(prefs.ntfy_server_url || "")
        setSavedNtfyUrl(prefs.ntfy_server_url || "")

        // Set auth type based on what's configured
        if (prefs.ntfy_has_access_token) {
          setNtfyAuthType("token")
        } else if (prefs.ntfy_has_credentials) {
          setNtfyAuthType("basic")
          setNtfyUsername(prefs.ntfy_username || "")
        } else {
          setNtfyAuthType("none")
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

  const handleNtfyServerSave = useCallback(async () => {
    setIsUpdatingNtfy(true)
    setNtfyError(null)
    setNtfySuccess(false)

    try {
      const result = await api.updateUserPreferences({ ntfy_server_url: ntfyServerUrl || "" })
      setUserPreferences(result)
      setSavedNtfyUrl(result.ntfy_server_url || "")
      setNtfySuccess(true)
      setTimeout(() => setNtfySuccess(false), 3000)
    } catch (error) {
      console.error("Failed to update ntfy server URL:", error)
      setNtfyError(error instanceof Error ? error.message : "Failed to save")
      setNtfyServerUrl(savedNtfyUrl)
    } finally {
      setIsUpdatingNtfy(false)
    }
  }, [ntfyServerUrl, savedNtfyUrl])

  const handleNtfyAuthSave = useCallback(async () => {
    setIsUpdatingNtfyAuth(true)
    setNtfyAuthError(null)
    setNtfyAuthSuccess(false)

    try {
      let updateData: {
        ntfy_access_token?: string
        ntfy_username?: string
        ntfy_password?: string
      } = {}

      if (ntfyAuthType === "none") {
        updateData = {
          ntfy_access_token: "",
          ntfy_username: "",
          ntfy_password: "",
        }
      } else if (ntfyAuthType === "token") {
        if (!ntfyAccessToken.trim()) {
          setNtfyAuthError("Access token is required")
          setIsUpdatingNtfyAuth(false)
          return
        }
        updateData = { ntfy_access_token: ntfyAccessToken.trim() }
      } else if (ntfyAuthType === "basic") {
        if (!ntfyUsername.trim() || !ntfyPassword.trim()) {
          setNtfyAuthError("Both username and password are required")
          setIsUpdatingNtfyAuth(false)
          return
        }
        updateData = {
          ntfy_username: ntfyUsername.trim(),
          ntfy_password: ntfyPassword.trim(),
        }
      }

      const result = await api.updateUserPreferences(updateData)
      setUserPreferences(result)

      // Clear sensitive fields after save
      setNtfyAccessToken("")
      setNtfyPassword("")

      setNtfyAuthSuccess(true)
      setTimeout(() => setNtfyAuthSuccess(false), 3000)
    } catch (error) {
      console.error("Failed to update ntfy authentication:", error)
      setNtfyAuthError(error instanceof Error ? error.message : "Failed to save")
    } finally {
      setIsUpdatingNtfyAuth(false)
    }
  }, [ntfyAuthType, ntfyAccessToken, ntfyUsername, ntfyPassword])

  const clearNtfyErrors = useCallback(() => {
    setNtfyError(null)
    setNtfySuccess(false)
  }, [])

  const clearNtfyAuthErrors = useCallback(() => {
    setNtfyAuthError(null)
    setNtfyAuthSuccess(false)
  }, [])

  return {
    // Regional settings
    currentLocale,
    selectedCurrency,
    isUpdatingCurrency,
    handleLanguageChange,
    handleCurrencyChange,

    // ntfy settings
    ntfyServerUrl,
    setNtfyServerUrl,
    hasNtfyChanges,
    isUpdatingNtfy,
    ntfyError,
    ntfySuccess,
    handleNtfyServerSave,
    clearNtfyErrors,

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
    isUpdatingNtfyAuth,
    ntfyAuthError,
    ntfyAuthSuccess,
    handleNtfyAuthSave,
    clearNtfyAuthErrors,
  }
}
