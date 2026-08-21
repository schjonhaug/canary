import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import CloudPageContent from '@/components/cloud-page'

export const metadata: Metadata = {
  title: 'Canary Cloud | Hosted Bitcoin Wallet Monitoring',
  description: 'Canary Cloud is the hosted Canary subscription for Bitcoin wallet monitoring, with clear privacy tradeoffs and no access to private keys.',
  alternates: {
    canonical: 'https://canarybitcoin.com/cloud',
  },
  openGraph: {
    title: 'Canary Cloud | Hosted Bitcoin Wallet Monitoring',
    description: 'Hosted Bitcoin wallet monitoring with clear privacy tradeoffs. Canary Cloud never receives your private keys.',
    url: 'https://canarybitcoin.com/cloud',
    images: ['/images/opengraph-image.png'],
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Canary Cloud | Hosted Bitcoin Wallet Monitoring',
    description: 'Hosted Bitcoin wallet monitoring with clear privacy tradeoffs and no access to private keys.',
    images: ['/images/x-image.png'],
  },
}

export default function CloudPage() {
  if (process.env.NEXT_PUBLIC_CANARY_MODE !== 'cloud') {
    notFound()
  }

  return <CloudPageContent />
}
