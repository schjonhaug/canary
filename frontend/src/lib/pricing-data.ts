// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, uncle_jim: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', uncle_jim: '5 wallets', unique: { uncle_jim: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', uncle_jim: '5 contacts per wallet', unique: { uncle_jim: true } },
  { id: 'sync', label: 'Sync interval', personal: '10 minute sync time', uncle_jim: '1 minute sync time', unique: { uncle_jim: true } },
  { id: 'email', label: 'Email notifications', personal: true, uncle_jim: true },
  { id: 'sms', label: 'SMS notifications', personal: true, uncle_jim: true },
  { id: 'push', label: 'Push notifications', personal: true, uncle_jim: true },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: true, uncle_jim: true },
]

// Feature mapping for Stripe metadata to display features
export const featureMapping: Record<string, { label: string, tiers: ('personal' | 'uncle_jim')[], unique?: Record<string, boolean> }> = {
  'wallets': { label: 'Bitcoin wallets', tiers: ['personal', 'uncle_jim'], unique: { uncle_jim: true } },
  'contacts_per_wallet': { label: 'Contacts per wallet', tiers: ['personal', 'uncle_jim'], unique: { uncle_jim: true } },
  'sync_interval': { label: 'Sync interval', tiers: ['personal', 'uncle_jim'], unique: { uncle_jim: true } },
  'email_notifications': { label: 'Email notifications', tiers: ['personal', 'uncle_jim'] },
  'sms_notifications': { label: 'SMS notifications', tiers: ['personal', 'uncle_jim'] },
  'push_notifications': { label: 'Push notifications', tiers: ['personal', 'uncle_jim'] },
  'transaction_analysis': { label: 'Transaction analysis (RBF/CPFP)', tiers: ['personal', 'uncle_jim'] },
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
    name: "Uncle Jim",
    slug: "uncle_jim", 
    monthlyPrice: 29,
    yearlyPrice: 278, // 20% discount: 29 * 12 * 0.8
    description: "For Uncle Jims & family guardians",
    cta: "Start Free Trial",
    ctaLink: "/sign-up",
    highlighted: true,
    badge: "POPULAR"
  }
]

// Type definitions
export type TierSlug = 'personal' | 'uncle_jim'
export type PricingTier = typeof pricingTiers[0]
export type Feature = typeof allFeatures[0]

// Helper functions
export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'Personal'
    case 'uncle_jim': return 'Uncle Jim'  
    default: return tier
  }
}

export function getTierDescription(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'For individual Bitcoin holders'
    case 'uncle_jim': return 'For Uncle Jims & family guardians'
    default: return ''
  }
}

