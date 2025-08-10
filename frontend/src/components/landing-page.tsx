'use client'

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Switch } from "@/components/ui/switch"
import { CheckCircle2, Zap, Shield, Bell, ArrowRight } from "lucide-react"
import Link from "next/link"
import Image from "next/image"

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

const allFeatures = [
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', pro: '5 wallets', business: 'Unlimited wallets', unique: { pro: true, business: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', pro: '10 contacts per wallet', business: 'Unlimited contacts', unique: { pro: true, business: true } },
  { id: 'email', label: 'Email notifications', personal: true, pro: true, business: true },
  { id: 'sms', label: 'SMS notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'push', label: 'Push notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'sync', label: 'Sync interval', personal: '10 minute sync time', pro: '1 minute sync time', business: '5 second sync time', unique: { pro: true, business: true } },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'api', label: 'REST API access', personal: false, pro: false, business: true, unique: { business: true } },
  { id: 'webhooks', label: 'Custom webhooks', personal: false, pro: false, business: true, unique: { business: true } },
  { id: 'support', label: 'Support', personal: 'Email support', pro: 'Priority email support', business: 'Dedicated support', unique: { pro: true, business: true } },
  { id: 'sla', label: '99.9% uptime SLA', personal: false, pro: false, business: true, unique: { business: true } },
]

const pricingTiers = [
  {
    name: "Personal",
    monthlyPrice: 0,
    yearlyPrice: 0,
    description: "Perfect for getting started",
    cta: "Sign up",
    ctaLink: "/sign-up",
    highlighted: false
  },
  {
    name: "Pro",
    monthlyPrice: 9,
    yearlyPrice: 86, // 20% discount: 9 * 12 * 0.8
    description: "Most popular for individuals",
    cta: "Subscribe",
    ctaLink: "/wallets",
    highlighted: true,
    badge: "POPULAR"
  },
  {
    name: "Business",
    monthlyPrice: 29,
    yearlyPrice: 278, // 20% discount: 29 * 12 * 0.8
    description: "For businesses and power users",
    cta: "Contact Sales",
    ctaLink: "mailto:mail@canarybitcoin.com",
    highlighted: false
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
  const [isYearly, setIsYearly] = useState(false)
  
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
                Get Started Free <ArrowRight className="ml-2 h-4 w-4" />
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
          <p className="text-muted-foreground">
            Start free, upgrade as you grow
          </p>
          
          {/* Billing Toggle */}
          <div className="flex items-center justify-center gap-3 mt-6">
            <span className={`text-sm ${!isYearly ? 'font-semibold' : 'text-muted-foreground'}`}>
              Monthly
            </span>
            <Switch
              checked={isYearly}
              onCheckedChange={setIsYearly}
              aria-label="Toggle yearly billing"
            />
            <span className={`text-sm ${isYearly ? 'font-semibold' : 'text-muted-foreground'}`}>
              Yearly
              {isYearly && (
                <span className="ml-1.5 text-xs bg-green-100 text-green-700 px-1.5 py-0.5 rounded-full">
                  Save 20%
                </span>
              )}
            </span>
          </div>
        </div>
        
        <div className="grid md:grid-cols-3 gap-6 max-w-5xl mx-auto">
          {pricingTiers.map((tier, index) => {
            const price = isYearly ? tier.yearlyPrice : tier.monthlyPrice
            const period = price === 0 ? '' : isYearly ? '/year' : '/month'
            const displayPrice = price === 0 ? 'Free' : `$${price}`
            
            return (
              <Card 
                key={index} 
                className={`relative ${tier.highlighted ? "border-primary shadow-md" : ""}`}
              >
                {tier.badge && (
                  <div className="absolute -top-3 left-4">
                    <span className="bg-primary text-primary-foreground text-xs px-2 py-1 rounded-full font-semibold">
                      {tier.badge}
                    </span>
                  </div>
                )}
                <CardHeader>
                  <CardTitle className="text-lg">{tier.name}</CardTitle>
                  <CardDescription className="text-sm">{tier.description}</CardDescription>
                  <div className="mt-3">
                    <span className="text-2xl font-bold">{displayPrice}</span>
                    {period && <span className="text-muted-foreground text-sm">{period}</span>}
                    {isYearly && tier.monthlyPrice > 0 && (
                      <div className="text-xs text-muted-foreground mt-1 line-through">
                        ${tier.monthlyPrice * 12}/year
                      </div>
                    )}
                  </div>
                </CardHeader>
                <CardContent>
                  <ul className="space-y-2.5">
                    {allFeatures.map((feature) => {
                      const tierKey = tier.name.toLowerCase() as 'personal' | 'pro' | 'business'
                      const value = feature[tierKey]
                      const isUnique = feature.unique?.[tierKey] || false
                      
                      if (value === false) {
                        return (
                          <li key={feature.id} className="flex items-start text-sm text-muted-foreground/50">
                            <span className="w-4 h-4 mr-2 flex-shrink-0 mt-0.5">–</span>
                            <span className="line-through">{feature.label}</span>
                          </li>
                        )
                      }
                      
                      return (
                        <li key={feature.id} className={`flex items-start text-sm ${isUnique && tier.name !== 'Personal' ? 'font-medium' : ''}`}>
                          <CheckCircle2 className={`h-4 w-4 mr-2 flex-shrink-0 mt-0.5 ${isUnique && tier.name !== 'Personal' ? 'text-primary' : 'text-muted-foreground'}`} />
                          <span>
                            {typeof value === 'string' ? value : feature.label}
                          </span>
                        </li>
                      )
                    })}
                  </ul>
                </CardContent>
                <CardFooter>
                  <Button 
                    className="w-full" 
                    variant={tier.highlighted ? "default" : "outline"}
                    asChild
                  >
                    <a href={tier.ctaLink}>
                      {tier.cta}
                    </a>
                  </Button>
                </CardFooter>
              </Card>
            )
          })}
        </div>
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
                  Get Started Free <ArrowRight className="ml-2 h-4 w-4" />
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