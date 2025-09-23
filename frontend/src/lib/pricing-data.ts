// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'trial', label: '30-day free trial', personal: true, team: true },
  { id: 'wallets', label: 'Bitcoin wallets', personal: '1 wallet', team: '5 wallets', unique: { team: true } },
  { id: 'contacts', label: 'Contacts per wallet', personal: '1 contact', team: '5 contacts per wallet', unique: { team: true } },
  { id: 'sync', label: 'Sync interval', personal: '10 minute sync time', team: '2 minute sync time', unique: { team: true } },
  { id: 'email', label: 'Email notifications', personal: true, team: true },
  { id: 'sms', label: 'SMS notifications', personal: true, team: true },
  { id: 'push', label: 'Push notifications', personal: true, team: true },
  { id: 'balance-alerts', label: 'Custom balance alerts', personal: true, team: true },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', personal: true, team: true },
]



// Type definitions
export type TierSlug = 'personal' | 'team'
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

