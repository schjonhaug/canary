"use client"

import { useEffect } from "react"
import { useRouter } from "next/navigation"
import { useAuth } from "@/contexts/auth-context"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { useTranslations } from "next-intl"
import { useUserPreferences } from "@/hooks/useUserPreferences"
import { RegionalSettings } from "@/components/settings/regional-settings"
import { NtfyServerSettings } from "@/components/settings/ntfy-server-settings"
import { ThemeSettings } from "@/components/settings/theme-settings"
import { TxExplorerSettings } from "@/components/settings/tx-explorer-settings"
import { NostrSettings } from "@/components/settings/nostr-settings"

export default function SettingsPage() {
  const router = useRouter()
  const t = useTranslations("settings")
  const tCommon = useTranslations("common")
  const { isAuthenticated, isLoading: authLoading, isCloudMode, user } = useAuth()

  const preferences = useUserPreferences({ isAuthenticated })

  // Redirect unauthenticated users to sign-in when in cloud mode
  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) {
      router.push("/sign-in")
    }
  }, [isCloudMode, isAuthenticated, authLoading, router])

  // Show loading state while auth is loading
  if (authLoading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon("loading")}</p>
        </div>
      </div>
    )
  }

  // Return null while redirecting unauthenticated users in cloud mode
  if (isCloudMode && !isAuthenticated) {
    return null
  }

  const isDisabled = isCloudMode && user?.is_demo === true

  return (
    <div className="space-y-6">
      <h2 className="text-2xl font-semibold">{t("title")}</h2>

      <div className="max-w-4xl space-y-6">
        <ThemeSettings />

        <RegionalSettings
          currentLocale={preferences.currentLocale}
          selectedCurrency={preferences.selectedCurrency}
          isUpdatingCurrency={preferences.isUpdatingCurrency}
          isDisabled={isDisabled}
          onLanguageChange={preferences.handleLanguageChange}
          onCurrencyChange={preferences.handleCurrencyChange}
        />

        {!isCloudMode && (
          <TxExplorerSettings
            explorers={preferences.availableTxExplorers}
            selectedExplorerId={preferences.selectedTxExplorerId}
            savedExplorerId={preferences.savedTxExplorerId}
            customExplorerUrl={preferences.customTxExplorerUrl}
            savedCustomExplorerUrl={preferences.savedCustomTxExplorerUrl}
            settingsError={preferences.txExplorerSettingsError}
            isUpdating={preferences.isUpdatingTxExplorer}
            onExplorerChange={preferences.handleTxExplorerChange}
            onCustomExplorerUrlChange={preferences.setCustomTxExplorerUrl}
            onCustomExplorerSave={preferences.handleCustomTxExplorerSave}
          />
        )}

        {!isCloudMode && (
          <NtfyServerSettings
            ntfyServerUrl={preferences.ntfyServerUrl}
            onNtfyServerUrlChange={preferences.setNtfyServerUrl}
            ntfyServers={preferences.availableNtfyServers}
            selectedNtfyServerId={preferences.selectedNtfyServerId}
            onNtfyServerChange={preferences.handleNtfyServerChange}
            userPreferences={preferences.userPreferences}
            ntfyAuthType={preferences.ntfyAuthType}
            onNtfyAuthTypeChange={preferences.setNtfyAuthType}
            ntfyAccessToken={preferences.ntfyAccessToken}
            onNtfyAccessTokenChange={preferences.setNtfyAccessToken}
            ntfyUsername={preferences.ntfyUsername}
            onNtfyUsernameChange={preferences.setNtfyUsername}
            ntfyPassword={preferences.ntfyPassword}
            onNtfyPasswordChange={preferences.setNtfyPassword}
            hasAnyNtfyChanges={preferences.hasAnyNtfyChanges}
            isUpdatingNtfySettings={preferences.isUpdatingNtfySettings}
            ntfySettingsError={preferences.ntfySettingsError}
            ntfySettingsSuccess={preferences.ntfySettingsSuccess}
            onNtfySettingsSave={preferences.handleNtfySettingsSave}
            onClearNtfySettingsErrors={preferences.clearNtfySettingsErrors}
          />
        )}

        {!isCloudMode && <NostrSettings />}
      </div>
    </div>
  )
}
