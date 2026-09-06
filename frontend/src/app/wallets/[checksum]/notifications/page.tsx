"use client"

import { Plus } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useParams, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { ContactCreationWizard } from "@/components/wallet-notifications/contact-wizard"
import { ContactEditor } from "@/components/wallet-notifications/contact-editor"
import { ContactSummaryCard } from "@/components/wallet-notifications/contact-summary-card"
import type { BalanceDraft } from "@/components/wallet-notifications/types"
import { formatBalanceDraft } from "@/components/wallet-notifications/balance-draft-controls"
import { PlansModal } from "@/components/plans-modal"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import {
  WalletDetailHeader,
  WalletDetailSkeleton,
  getWalletDetailErrorState,
} from "@/components/wallet-detail"
import { useAuth } from "@/contexts/auth-context"
import { useWalletsContext } from "@/contexts/wallets-context"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError, hasReachedContactLimit } from "@/lib/utils"
import type { BalanceAlert, Contact, Wallet } from "@/types"

type ActiveFlow = { type: "create" } | { type: "edit"; contactId: string } | null

export default function WalletNotificationsPage() {
  const params = useParams()
  const router = useRouter()
  const checksum = params.checksum as string
  const {
    user,
    billingStatus,
    isAuthenticated,
    isLoading: authLoading,
    isCloudMode,
    isSelfHostedMode,
  } = useAuth()
  const { setCurrentWallet } = useWalletsContext()
  const tWallets = useTranslations("wallets")
  const tCommon = useTranslations("common")
  const tApiErrors = useTranslations("errors.api")
  const t = useTranslations("walletNotifications")
  const [wallet, setWallet] = useState<Wallet | null>(null)
  const [contacts, setContacts] = useState<Contact[]>([])
  const [alerts, setAlerts] = useState<BalanceAlert[]>([])
  const [registeredProviderNames, setRegisteredProviderNames] = useState<string[]>([])
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [activeFlow, setActiveFlow] = useState<ActiveFlow>(null)
  const [showUpgradeModal, setShowUpgradeModal] = useState(false)
  const [preferredFiatCurrency, setPreferredFiatCurrency] = useState("USD")
  const [relativeTimeNow] = useState(() => Date.now())

  const load = async () => {
    setIsLoading(true)
    setError(null)
    try {
      const [data, preferences, providers] = await Promise.all([
        api.getWalletNotifications(checksum),
        api.getUserPreferences().catch(() => null),
        api.getProviders().catch(() => ({ providers: [] })),
      ])
      setWallet(data.wallet)
      setContacts(data.contacts)
      setAlerts(data.balance_alerts)
      setRegisteredProviderNames(providers.providers.map((provider) => provider.name))
      if (preferences?.preferred_fiat_currency) {
        setPreferredFiatCurrency(preferences.preferred_fiat_currency)
      }
      setCurrentWallet?.(data.wallet)
    } catch (caught) {
      setError(
        caught instanceof ApiError
          ? getTranslatedApiError(caught, tApiErrors)
          : t("errors.loadFailed")
      )
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    if (isCloudMode && !authLoading && !isAuthenticated) router.push("/sign-in")
  }, [authLoading, isAuthenticated, isCloudMode, router])

  useEffect(() => {
    if (!authLoading && (!isCloudMode || isAuthenticated)) void load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authLoading, isAuthenticated, isCloudMode, checksum])

  const alertsByContact = useMemo(() => alerts.reduce<Record<string, BalanceAlert[]>>((acc, alert) => {
    if (!alert.contact_id) return acc
    acc[alert.contact_id] = [...(acc[alert.contact_id] || []), alert]
    return acc
  }, {}), [alerts])
  const sortedContacts = useMemo(
    () => [...contacts].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
    [contacts]
  )
  const currentTier = billingStatus?.subscription_tier || user?.subscription_tier || "personal"
  const isCloudViewOnlyUser = isCloudMode && (user?.is_admin === true || user?.is_demo === true)
  const contactLimitReached = isCloudMode && hasReachedContactLimit(contacts.length, currentTier)

  const startCreation = () => {
    if (contactLimitReached) {
      setShowUpgradeModal(true)
      return
    }
    setNotice(null)
    setActiveFlow({ type: "create" })
  }

  if (authLoading || (isLoading && !wallet)) {
    return authLoading ? (
      <div className="flex h-screen items-center justify-center">
        <div className="text-center">
          <LoadingSpinner size="lg" className="mx-auto" />
          <p className="mt-4 text-gray-600">{tCommon("loading")}</p>
        </div>
      </div>
    ) : <WalletDetailSkeleton />
  }

  if (isCloudMode && !isAuthenticated) return null

  const errorState = getWalletDetailErrorState({
    error,
    wallet,
    checksum,
    t: tWallets,
    tCommon,
    canDelete: false,
    now: relativeTimeNow,
  })
  if (errorState) return errorState

  const reportCreationResult = (failed?: BalanceDraft[]) => {
    if (failed?.length) {
      setNotice(t("partial.create", {
        operations: failed.map((alert) => `${t(`alertTypes.${alert.alert_type}`)} ${formatBalanceDraft(alert)}`).join(", "),
      }))
    }
    setActiveFlow(null)
    void load()
  }

  return (
    <div className="space-y-6">
      <WalletDetailHeader walletChecksum={wallet!.checksum} walletName={wallet!.name} onNameUpdated={load} />

      <section className="space-y-4">
        <div className="flex items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">{t("title")}</h1>
            <p className="text-sm text-muted-foreground">{t("description")}</p>
          </div>
          {!activeFlow && !isCloudViewOnlyUser && (
            <Button type="button" onClick={startCreation}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              {t("addContact")}
            </Button>
          )}
        </div>

        {notice && (
          <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {notice}
          </p>
        )}

        <div className="space-y-4">
          {activeFlow?.type === "create" && (
            <ContactCreationWizard
              walletChecksum={wallet!.checksum}
              isSelfHostedMode={isSelfHostedMode}
              registeredProviderNames={registeredProviderNames}
              preferredFiatCurrency={preferredFiatCurrency}
              onCancel={() => setActiveFlow(null)}
              onCreated={reportCreationResult}
            />
          )}

          {contacts.length === 0 && !activeFlow ? (
            <Card>
              <CardContent className="py-8 text-center text-sm text-muted-foreground">
                {t("empty")}
              </CardContent>
            </Card>
          ) : sortedContacts.map((contact) => {
            const isEditing = activeFlow?.type === "edit" && activeFlow.contactId === contact.id
            if (isEditing) {
              return (
                <ContactEditor
                  key={contact.id}
                  contact={contact}
                  alerts={alertsByContact[contact.id] || []}
                  walletChecksum={wallet!.checksum}
                  isSelfHostedMode={isSelfHostedMode}
                  registeredProviderNames={registeredProviderNames}
                  preferredFiatCurrency={preferredFiatCurrency}
                  onCancel={() => setActiveFlow(null)}
                  onSaved={(failedOperations) => {
                    if (failedOperations) setNotice(t("partial.save", { operations: failedOperations.join(", ") }))
                    setActiveFlow(null)
                    void load()
                  }}
                />
              )
            }
            return (
              <ContactSummaryCard
                key={contact.id}
                contact={contact}
                alerts={alertsByContact[contact.id] || []}
                isSelfHostedMode={isSelfHostedMode}
                isReadOnly={isCloudViewOnlyUser || Boolean(activeFlow)}
                onEdit={() => { setNotice(null); setActiveFlow({ type: "edit", contactId: contact.id }) }}
                onDeleted={() => void load()}
              />
            )
          })}
        </div>
      </section>

      <PlansModal
        isOpen={showUpgradeModal}
        onClose={() => setShowUpgradeModal(false)}
        currentTier={currentTier}
        currentContactCount={contacts.length}
        limitType="contacts"
        isTrialUser={billingStatus?.subscription_status === "trialing"}
        billingStatus={billingStatus ? {
          subscription_status: billingStatus.subscription_status,
          stripe_customer_id: billingStatus.stripe_customer_id,
        } : undefined}
      />
    </div>
  )
}
