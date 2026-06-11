import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { AuthProvider } from "@/contexts/auth-context";
import { NextIntlClientProvider } from 'next-intl';
import { getLocale, getMessages } from 'next-intl/server';
import { getThemeInitializationScript } from "@/lib/theme";
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
  title: "Bitcoin Wallet Monitoring & Notifications | Canary Wallet - Watch-Only Bitcoin Tracker",
  description: "Professional Bitcoin wallet monitoring with instant email, SMS & push notifications. Watch-only access using XPUB descriptors - never touch your keys. 30-day free trial.",
  keywords: "bitcoin wallet monitoring, bitcoin transaction notifications, bitcoin wallet alerts, watch-only bitcoin wallet, bitcoin wallet tracker, xpub monitoring, bitcoin cold storage monitoring, bitcoin address monitoring",
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
    title: "Bitcoin Wallet Monitoring & Notifications | Canary Wallet",
    description: "Professional Bitcoin wallet monitoring with instant notifications. Watch-only access using XPUB descriptors. Never touch your private keys.",
    url: "https://canarybitcoin.com",
    siteName: "Canary Wallet",
    locale: "en_US",
    type: "website",
    images: ["/images/opengraph-image.png"],
  },
  twitter: {
    card: "summary_large_image",
    title: "Bitcoin Wallet Monitoring & Notifications",
    description: "Never miss a Bitcoin transaction with professional wallet monitoring. Instant email, SMS & push notifications.",
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

  const jsonLd = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    "name": "Canary Wallet Monitor",
    "applicationCategory": "FinanceApplication",
    "description": "Professional Bitcoin wallet monitoring service with instant notifications for transactions. Watch-only access using XPUB descriptors.",
    "operatingSystem": "Web-based",
    "offers": {
      "@type": "Offer",
      "price": "9",
      "priceCurrency": "USD",
      "priceSpecification": {
        "@type": "UnitPriceSpecification",
        "price": "9",
        "priceCurrency": "USD",
        "unitText": "monthly"
      }
    },
    "aggregateRating": {
      "@type": "AggregateRating",
      "ratingValue": "4.8",
      "reviewCount": "127"
    },
    "featureList": [
      "Bitcoin wallet monitoring",
      "Instant transaction notifications",
      "Email, SMS, and push notifications",
      "XPUB and descriptor support",
      "Watch-only security",
      "Deep address scanning",
      "RBF and CPFP detection",
      "Multi-wallet management",
      "30-day free trial"
    ]
  };

  return (
    <html lang={locale} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: getThemeInitializationScript() }} />
        <script
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
