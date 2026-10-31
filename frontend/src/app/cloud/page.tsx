import type { Metadata } from 'next'
import { headers } from 'next/headers'
import { notFound } from 'next/navigation'
import CloudPageContent from '@/components/cloud-page'
import { CSP_NONCE_HEADER } from '@/lib/content-security-policy'

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
    images: ['/images/opengraph-image.png'],
  },
}

export default async function CloudPage() {
  if (process.env.NEXT_PUBLIC_CANARY_MODE !== 'cloud') {
    notFound()
  }

  // The page's JSON-LD is an inline script, so it needs the request nonce like the root layout's.
  const nonce = (await headers()).get(CSP_NONCE_HEADER) ?? undefined

  return <CloudPageContent nonce={nonce} />
}
