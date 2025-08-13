// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, pro: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', pro: '15 wallets', unique: { pro: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', pro: '10 contacts per wallet', unique: { pro: true } },
  { id: 'sync', label: 'Sync interval', personal: '5 minute sync time', pro: '1 minute sync time', unique: { pro: true } },
  { id: 'email', label: 'Email notifications', personal: true, pro: true },
  { id: 'sms', label: 'SMS notifications', personal: false, pro: true, unique: { pro: true } },
  { id: 'push', label: 'Push notifications', personal: false, pro: true, unique: { pro: true } },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: false, pro: true, unique: { pro: true } },
]

// Feature mapping for Stripe metadata to display features
export const featureMapping: Record<string, { label: string, tiers: ('personal' | 'pro')[], unique?: Record<string, boolean> }> = {
  'wallets': { label: 'Bitcoin wallets', tiers: ['personal', 'pro'], unique: { pro: true } },
  'contacts_per_wallet': { label: 'Contacts per wallet', tiers: ['personal', 'pro'], unique: { pro: true } },
  'sync_interval': { label: 'Sync interval', tiers: ['personal', 'pro'], unique: { pro: true } },
  'email_notifications': { label: 'Email notifications', tiers: ['personal', 'pro'] },
  'sms_notifications': { label: 'SMS notifications', tiers: ['pro'], unique: { pro: true } },
  'push_notifications': { label: 'Push notifications', tiers: ['pro'], unique: { pro: true } },
  'transaction_analysis': { label: 'Transaction analysis (RBF/CPFP)', tiers: ['pro'], unique: { pro: true } },
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
  }
]

// Type definitions
export type TierSlug = 'personal' | 'pro'
export type PricingTier = typeof pricingTiers[0]
export type Feature = typeof allFeatures[0]

// Helper functions
export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'Personal'
    case 'pro': return 'Pro'  
    default: return tier
  }
}

export function getTierDescription(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'For individual Bitcoin holders'
    case 'pro': return 'For Uncle Jims & family guardians'
    default: return ''
  }
}

