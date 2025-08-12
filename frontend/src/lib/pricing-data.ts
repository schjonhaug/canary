// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, pro: true, business: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', pro: '15 wallets', business: 'Unlimited wallets', unique: { pro: true, business: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', pro: '10 contacts per wallet', business: 'Unlimited contacts', unique: { pro: true, business: true } },
  { id: 'sync', label: 'Sync interval', personal: '5 minute sync time', pro: '1 minute sync time', business: '5 second sync time', unique: { pro: true, business: true } },
  { id: 'email', label: 'Email notifications', personal: true, pro: true, business: true },
  { id: 'sms', label: 'SMS notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'push', label: 'Push notifications', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: false, pro: true, business: true, unique: { pro: true } },
  { id: 'api', label: 'REST API access', personal: false, pro: false, business: true, unique: { business: true } },
  { id: 'webhooks', label: 'Custom webhooks', personal: false, pro: false, business: true, unique: { business: true } },
]

// Feature mapping for Stripe metadata to display features
export const featureMapping: Record<string, { label: string, tiers: ('personal' | 'pro' | 'business')[], unique?: Record<string, boolean> }> = {
  'wallets': { label: 'Bitcoin wallets', tiers: ['personal', 'pro', 'business'], unique: { pro: true, business: true } },
  'contacts_per_wallet': { label: 'Contacts per wallet', tiers: ['personal', 'pro', 'business'], unique: { pro: true, business: true } },
  'sync_interval': { label: 'Sync interval', tiers: ['personal', 'pro', 'business'], unique: { pro: true, business: true } },
  'email_notifications': { label: 'Email notifications', tiers: ['personal', 'pro', 'business'] },
  'sms_notifications': { label: 'SMS notifications', tiers: ['pro', 'business'], unique: { pro: true } },
  'push_notifications': { label: 'Push notifications', tiers: ['pro', 'business'], unique: { pro: true } },
  'transaction_analysis': { label: 'Transaction analysis (RBF/CPFP)', tiers: ['pro', 'business'], unique: { pro: true } },
  'api_access': { label: 'REST API access', tiers: ['business'], unique: { business: true } },
  'webhooks': { label: 'Custom webhooks', tiers: ['business'], unique: { business: true } },
}

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

// Helper functions
export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'Personal'
    case 'pro': return 'Pro'  
    case 'business': return 'Business'
    default: return tier
  }
}

export function getTierDescription(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'For individual Bitcoin holders'
    case 'pro': return 'For Uncle Jims & family guardians'
    case 'business': return 'For businesses & services'
    default: return ''
  }
}

