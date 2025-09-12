'use client'

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Zap, Shield, Bell, ArrowRight } from "lucide-react"
import Link from "next/link"
import Image from "next/image"
import { PlanComparison } from "./plan-comparison"

const features = [
  {
    icon: <Bell className="h-5 w-5" />,
    title: "Instant Bitcoin Transaction Notifications",
    description: "Real-time Bitcoin wallet alerts via email, SMS and push notifications delivered straight to your device"
  },
  {
    icon: <Shield className="h-5 w-5" />,
    title: "Secure Watch-Only Bitcoin Monitoring",
    description: "Watch-only Bitcoin wallet access using XPUB descriptors - we never touch your private keys"
  },
  {
    icon: <Zap className="h-5 w-5" />,
    title: "Real-Time Bitcoin Wallet Sync",
    description: "Bitcoin wallet sync intervals from 10 minutes down to 2 minutes based on your subscription plan"
  }
]


const faqs = [
  {
    question: "What is Canary Bitcoin wallet monitoring?",
    answer: "Canary is a professional Bitcoin wallet monitoring and notification service that watches your Bitcoin wallets using XPUB descriptors and sends instant alerts for all transactions through email, SMS and push notifications. Perfect for monitoring cold storage, hardware wallets, and watch-only Bitcoin addresses."
  },
  {
    question: "How does Bitcoin wallet monitoring work without private keys?",
    answer: "Canary uses your wallet's XPUB (extended public key) or descriptor for watch-only Bitcoin monitoring. This gives us read-only access to track your Bitcoin addresses and detect transactions. We never have access to your private keys and cannot move your Bitcoin - ensuring complete security for your cold storage monitoring."
  },
  {
    question: "What is XPUB wallet monitoring?",
    answer: "XPUB monitoring allows you to track all addresses in your Bitcoin wallet without exposing private keys. By providing your wallet's XPUB or descriptor, Canary can monitor unlimited addresses, detect transactions at any depth (even 200+ addresses deep), and alert you instantly when Bitcoin moves."
  },
  {
    question: "Can I monitor Bitcoin cold storage wallets?",
    answer: "Yes! Canary is perfect for Bitcoin cold storage monitoring. Whether you're using hardware wallets like Ledger or Trezor, paper wallets, or any other cold storage solution, simply provide your XPUB or descriptor for secure watch-only monitoring with instant notifications."
  },
  {
    question: "How fast are Bitcoin transaction notifications?",
    answer: "With our Team plan, you get near real-time Bitcoin transaction notifications with sync intervals as fast as 2 minutes on mainnet. Most notifications arrive within seconds of transaction detection. Personal plans sync every 10 minutes, still ensuring timely alerts for your Bitcoin wallets."
  },
  {
    question: "What Bitcoin wallet types are supported?",
    answer: "Canary supports all major Bitcoin wallet types including Native SegWit (P2WPKH), Legacy (P2PKH), Nested SegWit (P2SH), and Taproot (P2TR) addresses. We automatically detect your wallet type from the XPUB or you can specify it manually. Multi-signature and hardware wallet monitoring are fully supported."
  },
  {
    question: "Why is it called Canary?",
    answer: "Like a canary in a coal mine providing early warning, Canary acts as your Bitcoin wallet's early warning system. When your Bitcoin is in cold storage, you rarely check on it. Canary monitors 24/7 and alerts you instantly when your Bitcoin moves, providing peace of mind for long-term holders."
  },
  {
    question: "Can I track Bitcoin without downloading the blockchain?",
    answer: "Yes! Canary connects to professional Electrum servers to monitor the Bitcoin blockchain for you. No need to run a full node or download the blockchain - just provide your XPUB and start receiving instant notifications for all Bitcoin wallet activity."
  }
]

export default function LandingPage() {
  
  return (
    <div className="min-h-screen">
      {/* Header */}
      <header className="container mx-auto px-4 py-6">
        <div className="flex justify-end">
          <Link href="/sign-in" className="text-sm text-muted-foreground hover:text-foreground">
            Already a user? Sign in
          </Link>
        </div>
      </header>

      {/* Hero Section */}
      <section className="container mx-auto px-4 py-16">
        <div className="text-center max-w-3xl mx-auto">
          <Image
            src="/images/canary.svg"
            alt="Canary Logo"
            width={80}
            height={80}
            className="mx-auto mb-6"
          />
          <h1 className="text-4xl font-bold tracking-tight mb-4">
            Never Miss a Bitcoin Transaction
          </h1>
          <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
            Professional Bitcoin wallet monitoring with instant notifications via email, SMS, and push. 
            Watch-only access using any wallet descriptor - we never touch your keys.
          </p>
          <div className="flex gap-3 justify-center flex-wrap">
            <Button size="lg" asChild>
              <Link href="/sign-up">
                Start 30-Day Free Trial <ArrowRight className="ml-2 h-4 w-4" />
              </Link>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <Link href="#pricing">
                View Pricing
              </Link>
            </Button>
          </div>
        </div>
      </section>


      {/* Features Grid */}
      <section className="container mx-auto px-4 py-12">
        <div className="text-center mb-10">
          <h2 className="text-2xl font-semibold mb-3">Professional Bitcoin Monitoring</h2>
          <p className="text-muted-foreground">
            Professional monitoring with read-only security
          </p>
        </div>
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4 max-w-5xl mx-auto">
          {features.map((feature, index) => (
            <Card key={index} className="border-muted">
              <CardHeader className="pb-3">
                <div className="flex items-center gap-3">
                  <div className="text-primary">{feature.icon}</div>
                  <CardTitle className="text-base">{feature.title}</CardTitle>
                </div>
              </CardHeader>
              <CardContent>
                <p className="text-sm text-muted-foreground">{feature.description}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </section>

      {/* Pricing Section */}
      <section className="container mx-auto px-4 py-16" id="pricing">
        <div className="text-center mb-10">
          <h2 className="text-2xl font-semibold mb-3">Simple, Transparent Pricing</h2>
          <p className="text-muted-foreground mb-2">
            Choose the plan that fits your needs
          </p>
          <p className="text-sm text-green-600 font-medium">
            ✓ All plans include a 30-day free trial • No credit card required
          </p>
        </div>
        
        <PlanComparison
          currentTier=""
          highlightUpgrades={false}
          showPricing={true}
          isModal={false}
          showCallToAction={true}
          showUnifiedTrialButton={true}
        />
      </section>


      {/* FAQ Section */}
      <section className="container mx-auto px-4 py-12">
        <div className="max-w-3xl mx-auto">
          <div className="text-center mb-10">
            <h2 className="text-2xl font-semibold mb-3">Frequently Asked Questions</h2>
          </div>
          <div className="space-y-4">
            {faqs.map((faq, index) => (
              <Card key={index}>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">{faq.question}</CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-sm text-muted-foreground">{faq.answer}</p>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      </section>

      {/* CTA Section */}
      <section className="container mx-auto px-4 py-16">
        <Card className="bg-primary text-primary-foreground max-w-3xl mx-auto">
          <CardContent className="text-center py-10">
            <h2 className="text-2xl font-semibold mb-3">
              Start Monitoring Your Bitcoin Today
            </h2>
            <p className="mb-6 opacity-90">
              Never miss another Bitcoin transaction
            </p>
            <div className="flex gap-3 justify-center flex-wrap">
              <Button size="lg" variant="secondary" asChild>
                <Link href="/sign-up">
                  Start 30-Day Free Trial <ArrowRight className="ml-2 h-4 w-4" />
                </Link>
              </Button>
              <Button size="lg" variant="outline" className="bg-transparent border-primary-foreground text-primary-foreground hover:bg-primary-foreground/10" asChild>
                <Link href="/sign-in">
                  Sign In
                </Link>
              </Button>
            </div>
          </CardContent>
        </Card>
      </section>
    </div>
  )
}