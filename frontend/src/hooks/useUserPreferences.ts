import { useState, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { api } from "@/lib/api"
import { getStoredLocale, setStoredLocale } from "@/lib/locale"
import type { Locale } from "@/i18n/config"
import { invalidateTxExplorerCache } from "@/hooks/useTxExplorer"
import { buildTxExplorerOptions, resolveSelectedTxExplorer, type TxExplorerOption } from "@/lib/tx-explorers"
import {
  PUBLIC_NTFY_SERVER_URL,
  buildNtfyServerOptions,
  resolveSelectedNtfyServer,
  type NtfyServerOption,
} from "@/lib/ntfy-servers"

export interface UserPreferences {
  preferred_fiat_currency: string
  preferred_tx_explorer_id: string | null
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
  const [availableTxExplorers, setAvailableTxExplorers] = useState<TxExplorerOption[]>([])
  const [defaultTxExplorerId, setDefaultTxExplorerId] = useState<string>("mempool-space")
  const [selectedTxExplorerId, setSelectedTxExplorerId] = useState<string>("mempool-space")
  const [isUpdatingTxExplorer, setIsUpdatingTxExplorer] = useState(false)

  // ntfy server state
  const [ntfyServerUrl, setNtfyServerUrl] = useState<string>("")
  const [savedNtfyUrl, setSavedNtfyUrl] = useState<string>("")
  const [availableNtfyServers, setAvailableNtfyServers] = useState<NtfyServerOption[]>([])
  const [defaultNtfyServerId, setDefaultNtfyServerId] = useState<string>("ntfy-sh")
  const [selectedNtfyServerId, setSelectedNtfyServerId] = useState<string>("ntfy-sh")

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

  // Derived state - any ntfy field has changed
  const hasAnyNtfyChanges =
    ntfyServerUrl !== savedNtfyUrl ||
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

  useEffect(() => {
    const fetchConfig = async () => {
      try {
        const config = await api.getConfig()
        const location = typeof window === "undefined"
          ? null
          : { protocol: window.location.protocol, hostname: window.location.hostname }
        const txOptions = buildTxExplorerOptions(config, location)
        setAvailableTxExplorers(txOptions)
        setDefaultTxExplorerId(config.default_tx_explorer_id)
        setAvailableNtfyServers(buildNtfyServerOptions(config))
        setDefaultNtfyServerId(config.default_ntfy_server_id)
      } catch (error) {
        console.error("Failed to fetch app config:", error)
      }
    }

    fetchConfig()
  }, [])

  useEffect(() => {
    if (availableTxExplorers.length === 0) return

    const selectedExplorer = resolveSelectedTxExplorer(
      availableTxExplorers,
      userPreferences?.preferred_tx_explorer_id ?? null,
      defaultTxExplorerId
    )
    setSelectedTxExplorerId(selectedExplorer.id)
  }, [availableTxExplorers, userPreferences?.preferred_tx_explorer_id, defaultTxExplorerId])

  useEffect(() => {
    if (availableNtfyServers.length === 0) return

    const selectedServer = resolveSelectedNtfyServer(
      availableNtfyServers,
      userPreferences?.ntfy_server_url ?? null,
      defaultNtfyServerId
    )
    setSelectedNtfyServerId(selectedServer.id)

    if (userPreferences?.ntfy_server_url) {
      setNtfyServerUrl(selectedServer.baseUrl)
      setSavedNtfyUrl(selectedServer.baseUrl)
      return
    }

    if (!ntfyServerUrl && !savedNtfyUrl) {
      setNtfyServerUrl(selectedServer.baseUrl)
      setSavedNtfyUrl(selectedServer.baseUrl)
    }
  }, [availableNtfyServers, userPreferences?.ntfy_server_url, defaultNtfyServerId, ntfyServerUrl, savedNtfyUrl])

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
      if (!ntfyServerUrl.trim()) {
        setNtfySettingsError("ntfy server URL is required")
        return
      }

      if (!ntfyServerUrl.startsWith("http://") && !ntfyServerUrl.startsWith("https://")) {
        setNtfySettingsError("ntfy server URL must start with http:// or https://")
        return
      }

      let updateData: {
        ntfy_server_url?: string
        ntfy_access_token?: string
        ntfy_username?: string
        ntfy_password?: string
      } = {
        ntfy_server_url: ntfyServerUrl || "",
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
      setUserPreferences(result)
      setSavedNtfyUrl(result.ntfy_server_url || ntfyServerUrl)
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
      setNtfyAuthType(savedNtfyAuthType)
      if (savedNtfyAuthType === "basic") {
        setNtfyUsername(savedNtfyUsername)
      }
    } finally {
      setIsUpdatingNtfySettings(false)
    }
  }, [ntfyServerUrl, savedNtfyUrl, ntfyAuthType, savedNtfyAuthType, ntfyAccessToken, ntfyUsername, savedNtfyUsername, ntfyPassword])

  const clearNtfySettingsErrors = useCallback(() => {
    setNtfySettingsError(null)
    setNtfySettingsSuccess(false)
  }, [])

  const handleNtfyServerChange = useCallback((serverId: string) => {
    const selectedServer = availableNtfyServers.find((server) => server.id === serverId)
    if (!selectedServer) return

    setSelectedNtfyServerId(serverId)
    if (selectedServer.isLocal) {
      setNtfyServerUrl(selectedServer.baseUrl)
    } else if (selectedNtfyServerId !== selectedServer.id) {
      setNtfyServerUrl(PUBLIC_NTFY_SERVER_URL)
    }
    clearNtfySettingsErrors()
  }, [availableNtfyServers, clearNtfySettingsErrors, selectedNtfyServerId])

  const handleTxExplorerChange = useCallback(async (explorerId: string) => {
    const previousExplorerId = selectedTxExplorerId
    setSelectedTxExplorerId(explorerId)
    setIsUpdatingTxExplorer(true)

    try {
      const result = await api.updateUserPreferences({
        preferred_tx_explorer_id: explorerId,
      })
      setUserPreferences(result)
      const selectedExplorer = resolveSelectedTxExplorer(
        availableTxExplorers,
        result.preferred_tx_explorer_id,
        defaultTxExplorerId
      )
      setSelectedTxExplorerId(selectedExplorer.id)
      invalidateTxExplorerCache()
    } catch (error) {
      console.error("Failed to update tx explorer preference:", error)
      setSelectedTxExplorerId(previousExplorerId)
    } finally {
      setIsUpdatingTxExplorer(false)
    }
  }, [availableTxExplorers, defaultTxExplorerId, selectedTxExplorerId])

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
    availableNtfyServers,
    selectedNtfyServerId,
    handleNtfyServerChange,
    hasAnyNtfyChanges,
    isUpdatingNtfySettings,
    ntfySettingsError,
    ntfySettingsSuccess,
    handleNtfySettingsSave,
    clearNtfySettingsErrors,

    // ntfy auth
    userPreferences,
    availableTxExplorers,
    selectedTxExplorerId,
    isUpdatingTxExplorer,
    handleTxExplorerChange,
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
