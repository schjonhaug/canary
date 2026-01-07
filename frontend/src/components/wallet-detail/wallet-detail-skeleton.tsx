"use client"

import Link from "next/link"
import { Card, CardContent } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { useTranslations } from "next-intl"

export function WalletDetailSkeleton() {
  const t = useTranslations("wallets")

  return (
    <div className="space-y-6">
      {/* Header Skeleton */}
      <section>
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-3 flex-wrap min-w-0">
            <div className="min-w-0">
              <nav className="flex items-center text-2xl text-muted-foreground min-w-0">
                <Link
                  href="/wallets"
                  className="hover:text-foreground font-semibold flex-shrink-0"
                >
                  {t("title")}
                </Link>
                <span className="mx-2 flex-shrink-0">/</span>
                <Skeleton className="h-8 w-48" />
              </nav>
            </div>
          </div>
        </div>
      </section>

      {/* Main Content Skeleton */}
      <section>
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Wallet Info Sidebar Skeleton */}
          <div className="lg:col-span-1">
            <Card>
              <CardContent className="space-y-6">
                <div>
                  <div className="text-sm font-medium text-muted-foreground mb-2">
                    {t("detail.balance")}
                  </div>
                  <Skeleton className="h-8 w-40" />
                  <Skeleton className="h-4 w-24 mt-1" />
                </div>

                <div className="pt-2 border-t">
                  <div className="flex items-center justify-between mb-2">
                    <div className="text-sm font-medium text-muted-foreground">
                      {t("detail.contacts")}
                    </div>
                  </div>
                  <Skeleton className="h-20 w-full" />
                </div>

                <div className="pt-2 border-t">
                  <Skeleton className="h-16 w-full" />
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Transactions Skeleton */}
          <div className="lg:col-span-2">
            <Card>
              <CardContent className="space-y-4 py-6">
                <Skeleton className="h-6 w-32 mb-4" />
                {[1, 2, 3].map((i) => (
                  <div key={i} className="space-y-2 pb-4 border-b last:border-0">
                    <div className="flex items-center justify-between">
                      <Skeleton className="h-5 w-24" />
                      <Skeleton className="h-5 w-32" />
                    </div>
                    <Skeleton className="h-4 w-full" />
                  </div>
                ))}
              </CardContent>
            </Card>
          </div>
        </div>
      </section>
    </div>
  )
}
