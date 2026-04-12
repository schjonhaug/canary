"use client"

import { AppHeader } from "@/components/app-header"
import { AppFooter } from "@/components/app-footer"
import { DemoBanner } from "@/components/demo-banner"

export default function DonationsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <DemoBanner />
      <AppHeader />
      {children}
      <AppFooter />
    </div>
  )
}
