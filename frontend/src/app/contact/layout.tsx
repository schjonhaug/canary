import type { Metadata } from 'next'
import ContactShell from './contact-shell'

// The shell is a client component, so page metadata lives here. Without it the
// page inherits the root canonical and Google treats it as a homepage duplicate.
export const metadata: Metadata = {
  title: 'Contact | Canary Wallet',
  description: 'Questions or feedback about Canary Wallet? Send us a message and we will get back to you.',
  alternates: {
    canonical: 'https://canarybitcoin.com/contact',
  },
  openGraph: {
    title: 'Contact | Canary Wallet',
    description: 'Questions or feedback about Canary Wallet? Send us a message.',
    url: 'https://canarybitcoin.com/contact',
    images: ['/images/opengraph-image.png'],
    siteName: 'Canary Wallet',
    locale: 'en_US',
    type: 'website',
  },
}

export default function ContactLayout({ children }: { children: React.ReactNode }) {
  return <ContactShell>{children}</ContactShell>
}
