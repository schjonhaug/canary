import type { Metadata } from 'next'
import DonationsShell from './donations-shell'

// The shell is a client component, so page metadata lives here. Without it the
// page inherits the root canonical and Google treats it as a homepage duplicate.
export const metadata: Metadata = {
  title: 'Support Canary Wallet | Donations',
  description: 'Canary Wallet is free to self-host. A one-time or recurring bitcoin donation helps keep it actively developed.',
  alternates: {
    canonical: 'https://canarybitcoin.com/donations',
  },
  openGraph: {
    title: 'Support Canary Wallet | Donations',
    description: 'Help keep Canary Wallet free and actively developed with a bitcoin donation.',
    url: 'https://canarybitcoin.com/donations',
    images: ['/images/og/donations.png'],
    siteName: 'Canary Wallet',
    locale: 'en_US',
    type: 'website',
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Support Canary Wallet | Donations',
    description: 'Help keep Canary Wallet free and actively developed with a bitcoin donation.',
    images: ['/images/og/donations.png'],
  },
}

export default function DonationsLayout({ children }: { children: React.ReactNode }) {
  return <DonationsShell>{children}</DonationsShell>
}
