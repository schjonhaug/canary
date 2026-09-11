"use client"

import { FormEvent, useEffect, useState } from "react"
import { useRouter } from "next/navigation"
import { useAuth } from "@/contexts/auth-context"
import { api, ApiError } from "@/lib/api"
import { Wallet } from "@/types"
import { WalletCards } from "@/components/wallet-cards"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ErrorDisplay } from "@/components/ui/error-display"
import { LoadingSpinner } from "@/components/ui/loading-spinner"
import { getTranslatedApiError } from "@/lib/utils"
import { useLocale, useTranslations } from "next-intl"

const MIN_REASON_CHARS = 8
const MAX_REASON_CHARS = 280

interface SupportGrant {
  target_user_id: string
  target_email: string
  reason: string
  expires_at: number
}

export default function SupportPage() {
  const t = useTranslations("supportPage")
  const tCommon = useTranslations("common")
  const tErrors = useTranslations("errors.api")
  const locale = useLocale()
  const router = useRouter()
  const { isAuthenticated, isLoading: authLoading, user, isCloudMode } = useAuth()
  const [email, setEmail] = useState("")
  const [reason, setReason] = useState("")
  const [grant, setGrant] = useState<SupportGrant | null>(null)
  const [wallets, setWallets] = useState<Wallet[]>([])
  const [timestamp, setTimestamp] = useState<number | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [isSaving, setIsSaving] = useState(false)
  const [error, setError] = useState("")

  useEffect(() => {
    document.title = `Canary Wallet - ${t("title")}`
  }, [t])

  useEffect(() => {
    if (authLoading) {
      return
    }
    if (!isAuthenticated) {
      router.push("/sign-in")
      return
    }
    if (!isCloudMode || !user?.is_admin) {
      router.push("/wallets")
    }
  }, [authLoading, isAuthenticated, isCloudMode, router, user?.is_admin])

  useEffect(() => {
    if (!isAuthenticated || !isCloudMode || !user?.is_admin) {
      return
    }
    let cancelled = false
    const load = async () => {
      setError("")
      try {
        const data = await api.getSupportAccess()
        if (cancelled) {
          return
        }
        setGrant(data.grant)
        setWallets(data.wallets)
        setTimestamp(data.timestamp)
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof ApiError ? getTranslatedApiError(err, tErrors) : tCommon("error"))
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false)
        }
      }
    }
    void load()
    return () => {
      cancelled = true
    }
    // Translator identity changes would retrigger this load and flash the form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isAuthenticated, isCloudMode, user?.is_admin])

  useEffect(() => {
    if (!grant) {
      return
    }
    const remainingMs = grant.expires_at * 1000 - Date.now()
    const timer = window.setTimeout(() => {
      setGrant(null)
      setWallets([])
      setEmail("")
      setReason("")
    }, Math.max(remainingMs, 0))
    return () => window.clearTimeout(timer)
  }, [grant])

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    setError("")
    setIsSaving(true)
    try {
      const data = await api.createSupportAccess(email.trim(), reason.trim())
      setGrant(data.grant)
      setWallets(data.wallets)
      setTimestamp(data.timestamp)
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tErrors) : tCommon("error"))
    } finally {
      setIsSaving(false)
    }
  }

  const handleEndAccess = async () => {
    setError("")
    setIsSaving(true)
    try {
      const data = await api.revokeSupportAccess()
      setGrant(data.grant)
      setWallets(data.wallets)
      setTimestamp(data.timestamp)
      setEmail("")
      setReason("")
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tErrors) : tCommon("error"))
    } finally {
      setIsSaving(false)
    }
  }

  if (authLoading || !isAuthenticated || !isCloudMode || !user?.is_admin) {
    return (
      <div className="flex h-64 items-center justify-center">
        <LoadingSpinner size="lg" />
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <LoadingSpinner size="lg" />
      </div>
    )
  }

  return (
    <div className="space-y-8">
      <Card className="max-w-xl">
        <CardHeader>
          <CardTitle>{t("title")}</CardTitle>
          <CardDescription>{t("description")}</CardDescription>
        </CardHeader>
        <CardContent>
          {error && (
            <ErrorDisplay message={error} variant="inline" className="mb-4" />
          )}
          {grant ? (
            <div className="space-y-4">
              <p className="text-sm font-medium">{t("activeTitle", { email: grant.target_email })}</p>
              <p className="text-sm text-muted-foreground">
                {t("expires", {
                  time: new Date(grant.expires_at * 1000).toLocaleTimeString(locale, {
                    hour: "2-digit",
                    minute: "2-digit",
                  }),
                })}
              </p>
              <p className="text-sm text-muted-foreground">{t("readOnly")}</p>
              <Button type="button" variant="outline" onClick={handleEndAccess} disabled={isSaving}>
                {t("endAccess")}
              </Button>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="support-email">{t("emailLabel")}</Label>
                <Input
                  id="support-email"
                  type="email"
                  autoComplete="off"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="support-reason">{t("reasonLabel")}</Label>
                <Input
                  id="support-reason"
                  value={reason}
                  minLength={MIN_REASON_CHARS}
                  maxLength={MAX_REASON_CHARS}
                  onChange={(event) => setReason(event.target.value)}
                  placeholder={t("reasonPlaceholder")}
                  required
                />
              </div>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? tCommon("loading") : t("submit")}
              </Button>
            </form>
          )}
        </CardContent>
      </Card>

      {grant && (
        wallets.length === 0 ? (
          <p className="text-muted-foreground">{t("empty")}</p>
        ) : (
          <WalletCards
            wallets={wallets}
            error={null}
            lastUpdate={timestamp}
            readOnly
          />
        )
      )}
    </div>
  )
}
