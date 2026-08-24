import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { AuthProvider } from "@/contexts/auth-context";
import { NextIntlClientProvider } from 'next-intl';
import { getLocale, getMessages } from 'next-intl/server';
import { headers } from 'next/headers';
import { getThemeInitializationScript } from "@/lib/theme";
import { CSP_NONCE_HEADER } from "@/lib/content-security-policy";
import { ThemeProvider } from "@/hooks/useTheme";
import { ErrorBoundary } from "@/components/error-boundary";
import { staticErrorBoundaryMessages } from "@/components/error-boundary-messages";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});


export const metadata: Metadata = {
  metadataBase: new URL('https://canarybitcoin.com'),
  title: "Canary Wallet | Private, Self-Hosted Bitcoin Monitoring",
  description: "Run Canary on your Bitcoin node for watch-only wallet monitoring and notifications through the channels you choose. Free to self host and never needs private keys.",
  keywords: "self-hosted bitcoin monitoring, bitcoin node app, bitcoin transaction notifications, watch-only bitcoin wallet, bitcoin wallet alerts, xpub monitoring, bitcoin descriptor monitoring, cold storage monitoring",
  authors: [{ name: "Canary Wallet" }],
  creator: "Canary Wallet",
  publisher: "Canary Wallet",
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      'max-video-preview': -1,
      'max-image-preview': 'large',
      'max-snippet': -1,
    },
  },
  openGraph: {
    title: "Canary Wallet | Private, Self-Hosted Bitcoin Monitoring",
    description: "Run Canary on your Bitcoin node and receive watch-only wallet alerts through the notification channels you choose. Private keys stay with you.",
    url: "https://canarybitcoin.com",
    siteName: "Canary Wallet",
    locale: "en_US",
    type: "website",
    images: ["/images/opengraph-image.png"],
  },
  twitter: {
    card: "summary_large_image",
    title: "Canary Wallet | Self-Hosted Bitcoin Monitoring",
    description: "Know when your bitcoin moves. Run Canary on your node without sharing private keys.",
    images: ["/images/x-image.png"],
    creator: "@canarybitcoin",
  },
  alternates: {
    canonical: "https://canarybitcoin.com",
  },
  verification: {
    google: "google-site-verification-code",
  },
};



export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const locale = await getLocale();
  const messages = await getMessages();
  const nonce = (await headers()).get(CSP_NONCE_HEADER) ?? undefined;

  const jsonLd = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    "name": "Canary Wallet",
    "applicationCategory": "FinanceApplication",
    "description": "Self-hosted Bitcoin wallet monitoring that runs on your node and uses watch-only wallet information without private keys.",
    "operatingSystem": "Linux, Umbrel, Start9, myNode",
    "featureList": [
      "Self-hosted Bitcoin wallet monitoring",
      "Transaction activity notifications",
      "Configurable notification channels",
      "XPUB, descriptor, and address support",
      "No private key access",
      "Balance condition alerts",
      "Wallet drain detection",
      "Multilingual notifications"
    ]
  };

  return (
    <html lang={locale} suppressHydrationWarning>
      <head>
        <script
          nonce={nonce}
          suppressHydrationWarning
          dangerouslySetInnerHTML={{ __html: getThemeInitializationScript() }}
        />
        <script
          nonce={nonce}
          suppressHydrationWarning
          type="application/ld+json"
          dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
        />
      </head>
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased font-sans`}
      >
        <ErrorBoundary messages={staticErrorBoundaryMessages}>
          <ThemeProvider>
            <NextIntlClientProvider locale={locale} messages={messages}>
              {/* The outer boundary stays static because it must also catch provider setup failures before i18n is usable. */}
              <ErrorBoundary>
                <AuthProvider>
                  {children}
                </AuthProvider>
              </ErrorBoundary>
            </NextIntlClientProvider>
          </ThemeProvider>
        </ErrorBoundary>
      </body>
    </html>
  );
}
