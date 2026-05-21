import { useState, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { api } from "@/lib/api"
import { getStoredLocale, setStoredLocale } from "@/lib/locale"
import type { Locale } from "@/i18n/config"
import { invalidateTxExplorerCache } from "@/hooks/useTxExplorer"
import {
  CUSTOM_TX_EXPLORER_ID,
  buildTxExplorerOptions,
  decodeCustomTxExplorerPreference,
  encodeCustomTxExplorerPreference,
  isValidCustomTxExplorerTemplate,
  resolveSelectedTxExplorer,
  type TxExplorerOption,
} from "@/lib/tx-explorers"
import {
  PUBLIC_NTFY_SERVER_URL,
  PUBLIC_NTFY_SERVER_ID,
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
  const t = useTranslations("settings")
  const customTxExplorerName = t("txExplorer.custom.title")

  // Regional settings state
  const [selectedCurrency, setSelectedCurrency] = useState<string>("USD")
  const [currentLocale, setCurrentLocale] = useState<Locale>("en-US")
  const [isUpdatingCurrency, setIsUpdatingCurrency] = useState(false)

  // User preferences from API
  const [userPreferences, setUserPreferences] = useState<UserPreferences | null>(null)
  const [availableTxExplorers, setAvailableTxExplorers] = useState<TxExplorerOption[]>([])
  const [defaultTxExplorerId, setDefaultTxExplorerId] = useState<string>("mempool-space")
  const [selectedTxExplorerId, setSelectedTxExplorerId] = useState<string>("mempool-space")
  const [savedTxExplorerId, setSavedTxExplorerId] = useState<string>("mempool-space")
  const [customTxExplorerUrl, setCustomTxExplorerUrl] = useState<string>("")
  const [savedCustomTxExplorerUrl, setSavedCustomTxExplorerUrl] = useState<string>("")
  const [txExplorerSettingsError, setTxExplorerSettingsError] = useState<string | null>(null)
  const [isUpdatingTxExplorer, setIsUpdatingTxExplorer] = useState(false)

  // ntfy server state
  const [ntfyServerUrl, setNtfyServerUrl] = useState<string>(PUBLIC_NTFY_SERVER_URL)
  const [savedNtfyUrl, setSavedNtfyUrl] = useState<string>(PUBLIC_NTFY_SERVER_URL)
  const [availableNtfyServers, setAvailableNtfyServers] = useState<NtfyServerOption[]>([])
  const [defaultNtfyServerId, setDefaultNtfyServerId] = useState<string>(PUBLIC_NTFY_SERVER_ID)
  const [selectedNtfyServerId, setSelectedNtfyServerId] = useState<string>(PUBLIC_NTFY_SERVER_ID)
  const [savedNtfyServerId, setSavedNtfyServerId] = useState<string>(PUBLIC_NTFY_SERVER_ID)
  const [lastPublicNtfyUrl, setLastPublicNtfyUrl] = useState<string>(PUBLIC_NTFY_SERVER_URL)

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
    selectedNtfyServerId !== savedNtfyServerId ||
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
      defaultTxExplorerId,
      customTxExplorerName
    )
    setSelectedTxExplorerId(selectedExplorer.id)
    setSavedTxExplorerId(selectedExplorer.id)
    const customTemplate = decodeCustomTxExplorerPreference(userPreferences?.preferred_tx_explorer_id ?? null)
    if (customTemplate) {
      setCustomTxExplorerUrl(customTemplate)
      setSavedCustomTxExplorerUrl(customTemplate)
    } else {
      setCustomTxExplorerUrl("")
      setSavedCustomTxExplorerUrl("")
    }
  }, [availableTxExplorers, userPreferences?.preferred_tx_explorer_id, defaultTxExplorerId, customTxExplorerName])

  useEffect(() => {
    const hasLoadedPreferences = !isAuthenticated || userPreferences !== null
    if (availableNtfyServers.length === 0 || !hasLoadedPreferences) return

    const selectedServer = resolveSelectedNtfyServer(
      availableNtfyServers,
      userPreferences?.ntfy_server_url ?? null,
      defaultNtfyServerId
    )
    setSelectedNtfyServerId(selectedServer.id)
    setSavedNtfyServerId(selectedServer.id)
    setNtfyServerUrl(selectedServer.baseUrl)
    setSavedNtfyUrl(selectedServer.baseUrl)
    if (!selectedServer.isLocal) {
      setLastPublicNtfyUrl(selectedServer.baseUrl)
    }
  }, [availableNtfyServers, userPreferences, defaultNtfyServerId, isAuthenticated])

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
      if (availableNtfyServers.length === 0) {
        setNtfySettingsError(t("ntfy.validation.saveFailed"))
        return false
      }

      const selectedNtfyServer = availableNtfyServers.find((server) => server.id === selectedNtfyServerId)
      const normalizedNtfyServerUrl = ntfyServerUrl.trim().replace(/\/+$/, "")
      // Local integrations are detected from config on each load. Persist an empty preference so we
      // do not store Docker-internal URLs, while still letting the resolver reselect the local server.
      const ntfyUrlToStore = selectedNtfyServer?.isLocal ? "" : normalizedNtfyServerUrl

      if (!selectedNtfyServer?.isLocal && !normalizedNtfyServerUrl) {
        setNtfySettingsError(t("ntfy.validation.urlRequired"))
        return false
      }

      if (
        !selectedNtfyServer?.isLocal &&
        !normalizedNtfyServerUrl.startsWith("http://") &&
        !normalizedNtfyServerUrl.startsWith("https://")
      ) {
        setNtfySettingsError(t("ntfy.validation.urlProtocol"))
        return false
      }

      let updateData: {
        ntfy_server_url?: string
        ntfy_access_token?: string
        ntfy_username?: string
        ntfy_password?: string
      } = {
        ntfy_server_url: ntfyUrlToStore,
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
            setNtfySettingsError(t("ntfy.auth.tokenRequired"))
            return false
          }
          updateData = { ...updateData, ntfy_access_token: ntfyAccessToken.trim() }
        } else if (ntfyAuthType === "basic") {
          if (!ntfyUsername.trim() || !ntfyPassword.trim()) {
            setNtfySettingsError(t("ntfy.auth.credentialsRequired"))
            return false
          }
          updateData = {
            ...updateData,
            ntfy_username: ntfyUsername.trim(),
            ntfy_password: ntfyPassword.trim(),
          }
        }
      }

      const result = await api.updateUserPreferences(updateData)
      const savedUrlAfterSave = selectedNtfyServer?.isLocal
        ? selectedNtfyServer.baseUrl
        : result.ntfy_server_url || normalizedNtfyServerUrl || PUBLIC_NTFY_SERVER_URL
      setUserPreferences(result)
      setSavedNtfyServerId(selectedNtfyServer?.id ?? selectedNtfyServerId)
      setSavedNtfyUrl(savedUrlAfterSave)
      if (!selectedNtfyServer?.isLocal) {
        setNtfyServerUrl(savedUrlAfterSave)
        setLastPublicNtfyUrl(savedUrlAfterSave)
      }
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
      return true
    } catch (error) {
      console.error("Failed to update ntfy settings:", error)
      setNtfySettingsError(error instanceof Error ? error.message : t("ntfy.validation.saveFailed"))
      // Revert all fields to saved state
      setSelectedNtfyServerId(savedNtfyServerId)
      setNtfyServerUrl(savedNtfyUrl)
      setNtfyAuthType(savedNtfyAuthType)
      if (savedNtfyAuthType === "basic") {
        setNtfyUsername(savedNtfyUsername)
      }
      return false
    } finally {
      setIsUpdatingNtfySettings(false)
    }
  }, [ntfyServerUrl, savedNtfyUrl, ntfyAuthType, savedNtfyAuthType, ntfyAccessToken, ntfyUsername, savedNtfyUsername, ntfyPassword, availableNtfyServers, selectedNtfyServerId, savedNtfyServerId, t])

  const clearNtfySettingsErrors = useCallback(() => {
    setNtfySettingsError(null)
    setNtfySettingsSuccess(false)
  }, [])

  const handleNtfyServerChange = useCallback((serverId: string) => {
    const selectedServer = availableNtfyServers.find((server) => server.id === serverId)
    if (!selectedServer) return

    setSelectedNtfyServerId(serverId)
    if (selectedServer.isLocal) {
      const currentServer = availableNtfyServers.find((server) => server.id === selectedNtfyServerId)
      if (currentServer && !currentServer.isLocal && ntfyServerUrl.trim()) {
        setLastPublicNtfyUrl(ntfyServerUrl)
      }
      setNtfyServerUrl(selectedServer.baseUrl)
    } else {
      setNtfyServerUrl(lastPublicNtfyUrl)
    }
    clearNtfySettingsErrors()
  }, [availableNtfyServers, clearNtfySettingsErrors, lastPublicNtfyUrl, ntfyServerUrl, selectedNtfyServerId])

  const handleTxExplorerChange = useCallback(async (explorerId: string) => {
    setTxExplorerSettingsError(null)
    if (explorerId === CUSTOM_TX_EXPLORER_ID) {
      setSelectedTxExplorerId(CUSTOM_TX_EXPLORER_ID)
      return
    }

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
        defaultTxExplorerId,
        customTxExplorerName
      )
      setSelectedTxExplorerId(selectedExplorer.id)
      setSavedTxExplorerId(selectedExplorer.id)
      setCustomTxExplorerUrl("")
      setSavedCustomTxExplorerUrl("")
      invalidateTxExplorerCache()
    } catch (error) {
      console.error("Failed to update tx explorer preference:", error)
      setSelectedTxExplorerId(previousExplorerId)
    } finally {
      setIsUpdatingTxExplorer(false)
    }
  }, [availableTxExplorers, defaultTxExplorerId, selectedTxExplorerId, customTxExplorerName])

  const handleCustomTxExplorerSave = useCallback(async () => {
    const previousExplorerId = selectedTxExplorerId
    const normalizedCustomUrl = customTxExplorerUrl.trim()
    setTxExplorerSettingsError(null)

    if (!isValidCustomTxExplorerTemplate(normalizedCustomUrl)) {
      setTxExplorerSettingsError(t("txExplorer.custom.validation"))
      return false
    }

    setSelectedTxExplorerId(CUSTOM_TX_EXPLORER_ID)
    setIsUpdatingTxExplorer(true)

    try {
      const result = await api.updateUserPreferences({
        preferred_tx_explorer_id: encodeCustomTxExplorerPreference(normalizedCustomUrl),
      })
      setUserPreferences(result)
      setCustomTxExplorerUrl(normalizedCustomUrl)
      setSavedCustomTxExplorerUrl(normalizedCustomUrl)
      setSavedTxExplorerId(CUSTOM_TX_EXPLORER_ID)
      invalidateTxExplorerCache()
      return true
    } catch (error) {
      console.error("Failed to update custom tx explorer preference:", error)
      setTxExplorerSettingsError(t("txExplorer.custom.saveFailed"))
      setSelectedTxExplorerId(previousExplorerId)
      return false
    } finally {
      setIsUpdatingTxExplorer(false)
    }
  }, [customTxExplorerUrl, selectedTxExplorerId, t])

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
    savedTxExplorerId,
    customTxExplorerUrl,
    setCustomTxExplorerUrl,
    savedCustomTxExplorerUrl,
    txExplorerSettingsError,
    isUpdatingTxExplorer,
    handleTxExplorerChange,
    handleCustomTxExplorerSave,
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
