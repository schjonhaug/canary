'use client'

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Zap, Shield, Bell, ArrowRight } from "lucide-react"
import Link from "next/link"
import Image from "next/image"
import { PlanComparison } from "./plan-comparison"

const features = [
  {
    icon: <Bell className="h-5 w-5" />,
    title: "Instant Notifications",
    description: "Real-time alerts via email, SMS and push notifications delivered straight to your device"
  },
  {
    icon: <Shield className="h-5 w-5" />,
    title: "Read-Only Monitoring",
    description: "Watch-only access using descriptors - we never touch your keys"
  },
  {
    icon: <Zap className="h-5 w-5" />,
    title: "Fast Sync",
    description: "Sync intervals from 10 minutes down to 5 seconds based on your plan"
  }
]


const faqs = [
  {
    question: "What is Canary?",
    answer: "Canary is a professional Bitcoin wallet notification service that monitors your wallets and sends instant alerts for all transactions through email, SMS and push notifications. Never miss an important transaction again."
  },
  {
    question: "Why is it called Canary?",
    answer: "A canary in the coal mine - When your bitcoins are in cold storage, you seldom check on them. Canary acts as an early warning system that alerts you the moment your coins move, giving you immediate notification of any activity on your wallets."
  },
  {
    question: "Is it safe to use?",
    answer: "Yes! Canary only requires your wallet descriptor (xpub) for monitoring. This gives us read-only access to watch your addresses. We never have access to your private keys and cannot move your funds."
  },

  {
    question: "How fast are notifications?",
    answer: "With our Pro and Business plans, you get near real-time notifications with sync intervals as fast as 5 seconds. Most notifications arrive within seconds of transaction confirmation."
  },
  {
    question: "Can I upgrade or downgrade anytime?",
    answer: "Yes! You can change your plan at any time. Upgrades take effect immediately, and downgrades apply at the end of your billing cycle."
  },
  {
    question: "Is there an API available?",
    answer: "Business plan subscribers get full REST API access for integrating Canary into their existing systems and workflows."
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
          onContactSales={() => {
            window.location.href = 'mailto:sales@canarybitcoin.com?subject=Business Plan Inquiry&body=Hi, I am interested in the Business plan for Canary. Please contact me to discuss.'
          }}
          highlightUpgrades={false}
          showPricing={true}
          showBillingToggle={true}
          isModal={false}
          showCallToAction={true}
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