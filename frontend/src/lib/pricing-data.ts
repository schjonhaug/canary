// Shared pricing data and features - single source of truth
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, pro: true, business: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', pro: '15 wallets', business: 'Unlimited wallets', unique: { pro: true, business: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', pro: '10 contacts per wallet', business: 'Unlimited contacts', unique: { pro: true, business: true } },
  { id: 'email', label: 'Email notifications', personal: true, pro: true, business: true },
  { id: 'sms', label: 'SMS notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'push', label: 'Push notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'sync', label: 'Sync interval', personal: '5 minute sync time', pro: '1 minute sync time', business: '5 second sync time', unique: { pro: true, business: true } },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'api', label: 'REST API access', personal: false, pro: false, business: true, unique: { business: true } },
  { id: 'webhooks', label: 'Custom webhooks', personal: false, pro: false, business: true, unique: { business: true } },
  { id: 'support', label: 'Support', personal: 'Email support', pro: 'Priority support', business: 'Dedicated support', unique: { pro: true, business: true } },
  { id: 'sla', label: '99.9% uptime SLA', personal: false, pro: false, business: true, unique: { business: true } },
]

export const pricingTiers = [
  {
    name: "Personal",
    slug: "personal",
    monthlyPrice: 9,
    yearlyPrice: 86, // 20% discount: 9 * 12 * 0.8
    description: "For individual Bitcoin holders",
    cta: "Start Free Trial",
    ctaLink: "/sign-up",
    highlighted: false
  },
  {
    name: "Pro",
    slug: "pro", 
    monthlyPrice: 29,
    yearlyPrice: 278, // 20% discount: 29 * 12 * 0.8
    description: "For Uncle Jims & family guardians",
    cta: "Start Free Trial",
    ctaLink: "/sign-up",
    highlighted: true,
    badge: "POPULAR"
  },
  {
    name: "Business",
    slug: "business",
    monthlyPrice: 99,
    yearlyPrice: 950, // 20% discount: 99 * 12 * 0.8
    description: "For businesses & services",
    cta: "Contact Sales", // Different CTA for Business tier
    ctaLink: "mailto:sales@canarybitcoin.com", // Could be changed to a contact form
    highlighted: false
  }
]

// Type definitions
export type TierSlug = 'personal' | 'pro' | 'business'
export type PricingTier = typeof pricingTiers[0]
export type Feature = typeof allFeatures[0]