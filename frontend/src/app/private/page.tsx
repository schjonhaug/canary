import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import PrivatePageContent from '@/components/private-page'

export const metadata: Metadata = {
  title: 'Canary Private | Your Own Hosted Bitcoin Node',
  description: 'A dedicated Canary instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
  alternates: {
    canonical: 'https://canarybitcoin.com/private',
  },
  openGraph: {
    title: 'Canary Private | Your Own Hosted Bitcoin Node',
    description: 'A dedicated Canary instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
    url: 'https://canarybitcoin.com/private',
    images: ['/images/og/private.png'],
    siteName: 'Canary Wallet',
    locale: 'en_US',
    type: 'website',
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Canary Private | Your Own Hosted Bitcoin Node',
    description: 'A dedicated Canary instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
    images: ['/images/og/private.png'],
  },
}

export default function PrivatePage() {
  if (process.env.NEXT_PUBLIC_CANARY_MODE !== 'cloud') {
    notFound()
  }

  return <PrivatePageContent />
}
