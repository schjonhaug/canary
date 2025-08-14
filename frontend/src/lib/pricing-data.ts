// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, team: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', team: '5 wallets', unique: { team: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', team: '5 contacts per wallet', unique: { team: true } },
  { id: 'sync', label: 'Sync interval', personal: '10 minute sync time', team: '1 minute sync time', unique: { team: true } },
  { id: 'email', label: 'Email notifications', personal: true, team: true },
  { id: 'sms', label: 'SMS notifications', personal: true, team: true },
  { id: 'push', label: 'Push notifications', personal: true, team: true },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: true, team: true },
]

// Feature mapping for Stripe metadata to display features
export const featureMapping: Record<string, { label: string, tiers: ('personal' | 'team')[], unique?: Record<string, boolean> }> = {
  'wallets': { label: 'Bitcoin wallets', tiers: ['personal', 'team'], unique: { team: true } },
  'contacts_per_wallet': { label: 'Contacts per wallet', tiers: ['personal', 'team'], unique: { team: true } },
  'sync_interval': { label: 'Sync interval', tiers: ['personal', 'team'], unique: { team: true } },
  'email_notifications': { label: 'Email notifications', tiers: ['personal', 'team'] },
  'sms_notifications': { label: 'SMS notifications', tiers: ['personal', 'team'] },
  'push_notifications': { label: 'Push notifications', tiers: ['personal', 'team'] },
  'transaction_analysis': { label: 'Transaction analysis (RBF/CPFP)', tiers: ['personal', 'team'] },
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
    name: "Team",
    slug: "team", 
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
export type TierSlug = 'personal' | 'team'
export type PricingTier = typeof pricingTiers[0]
export type Feature = typeof allFeatures[0]

// Helper functions
export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'Personal'
    case 'team': return 'Team'  
    default: return tier
  }
}

export function getTierDescription(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal': return 'For individual Bitcoin holders'
    case 'team': return 'For Uncle Jims & family guardians'
    default: return ''
  }
}

