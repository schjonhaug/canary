"use client"

import Link from "next/link"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Plus, Clock } from "lucide-react"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

interface User {
  name?: string
  email: string
}

interface WalletOnboardingProps {
  user?: User | null
}

export function WalletOnboarding({ user }: WalletOnboardingProps) {
  const { billingStatus, isCloudMode } = useAuth()
  const isPending = isCloudMode && billingStatus?.subscription_status === 'pending'
  const t = useTranslations('wallets.onboarding')

  // If user is in pending status (cloud mode only), show trial activation message
  if (isPending) {
    return (
      <div className="max-w-3xl mx-auto mt-16">
        <Card className="border-muted">
          <CardContent className="pt-12 pb-10 px-8">
            <div className="space-y-8">
              {/* Trial Benefits Section */}
              <div className="text-center space-y-6">
                <div className="inline-flex items-center justify-center w-16 h-16 bg-blue-100 dark:bg-blue-900/50 rounded-full mb-4">
                  <Clock className="w-8 h-8 text-blue-600 dark:text-blue-400" />
                </div>

                <h2 className="text-3xl font-bold">
                  {user?.name ? t('trial.titleWithName', { name: user.name }) : t('trial.title')}
                </h2>

                <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                  {t.rich('trial.description', {
                    highlight: (chunks) => <span className="font-semibold text-blue-600 dark:text-blue-400">{chunks}</span>
                  })}
                </p>

                <div className="bg-gradient-to-br from-blue-50 to-indigo-50 dark:from-blue-950/30 dark:to-indigo-950/30 rounded-lg p-6 max-w-2xl mx-auto">
                  <h3 className="font-semibold text-lg text-blue-800 dark:text-blue-300 mb-3">{t('trial.whatYouGet')}</h3>
                  <div className="grid md:grid-cols-2 gap-3 text-sm text-blue-700 dark:text-blue-400">
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      {t('trial.features.wallets')}
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      {t('trial.features.contacts')}
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      {t('trial.features.emailSms')}
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      {t('trial.features.push')}
                    </div>
                  </div>
                </div>
              </div>

              <div className="text-center pt-4">
                <Link href="/wallets/add">
                  <Button
                    size="lg"
                    className="bg-blue-600 hover:bg-blue-700 text-white gap-2 text-lg px-8 py-3"
                  >
                    <Plus size={20} />
                    {t('trial.button')}
                  </Button>
                </Link>
                <p className="text-sm text-muted-foreground mt-3">
                  {t('trial.note')}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  // For non-pending users with no wallets (both self-hosted and cloud mode)
  return (
    <div className="max-w-lg mx-auto mt-16">
      <Card className="border-muted">
        <CardContent className="pt-12 pb-10 px-8">
          <div className="text-center space-y-6">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-accent/10 rounded-full mb-4">
              <Plus className="w-8 h-8 text-accent" />
            </div>

            <h2 className="text-2xl font-bold">
              {t('simple.title')}
            </h2>

            <p className="text-muted-foreground">
              {t('simple.description')}
            </p>

            <div className="pt-4">
              <Link href="/wallets/add">
                <Button
                  size="lg"
                  className="bg-accent hover:bg-accent/90 text-accent-foreground gap-2"
                >
                  <Plus size={20} />
                  {t('simple.button')}
                </Button>
              </Link>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
