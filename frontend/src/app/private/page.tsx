import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import PrivatePageContent from '@/components/private-page'

export const metadata: Metadata = {
  title: 'Canary Wallet Private | Your Own Hosted Bitcoin Node',
  description: 'A dedicated Canary Wallet instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
  alternates: {
    canonical: 'https://canarybitcoin.com/private',
  },
  openGraph: {
    title: 'Canary Wallet Private | Your Own Hosted Bitcoin Node',
    description: 'A dedicated Canary Wallet instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
    url: 'https://canarybitcoin.com/private',
    images: ['/images/opengraph-image.png'],
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Canary Wallet Private | Your Own Hosted Bitcoin Node',
    description: 'A dedicated Canary Wallet instance and full Bitcoin node in your own hosting account, with personal onboarding and ongoing maintenance support. Available by arrangement.',
    images: ['/images/x-image.png'],
  },
}

export default function PrivatePage() {
  if (process.env.NEXT_PUBLIC_CANARY_WALLET_MODE !== 'cloud') {
    notFound()
  }

  return <PrivatePageContent />
}
