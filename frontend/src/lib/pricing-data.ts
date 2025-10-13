// Shared feature definitions - used for both static and dynamic pricing
export const allFeatures = [
  { id: 'wallets', label: 'Bitcoin wallets', selfhosted: 'Unlimited wallets', personal: '1 wallet', team: '5 wallets', unique: { selfhosted: true, team: true } },
  { id: 'contacts', label: 'Contacts per wallet', selfhosted: 'Unlimited contacts', personal: '1 contact', team: '5 contacts per wallet', unique: { selfhosted: true, team: true } },
  { id: 'sync', label: 'Sync interval', selfhosted: 'Configurable sync time', personal: '10 minute sync time', team: '2 minute sync time', unique: { selfhosted: true, team: true } },
  { id: 'trial', label: '30-day free trial', personal: true, team: true },
  { id: 'email', label: 'Email notifications', selfhosted: false, personal: true, team: true },
  { id: 'sms', label: 'SMS notifications', selfhosted: false, personal: true, team: true },
  { id: 'push', label: 'Push notifications', selfhosted: true, personal: true, team: true },
  { id: 'balance-alerts', label: 'Custom balance alerts', selfhosted: true, personal: true, team: true },
  { id: 'analysis', label: 'Transaction analysis (RBF/CPFP)', selfhosted: true, personal: true, team: true },
  { id: 'own-node', label: 'Your own Bitcoin node', selfhosted: true, unique: { selfhosted: true } },
  { id: 'privacy', label: 'Complete privacy & control', selfhosted: true, unique: { selfhosted: true } },
  { id: 'subscription', label: 'No subscription fees', selfhosted: true, unique: { selfhosted: true } },
]



// Type definitions
export type TierSlug = 'selfhosted' | 'personal' | 'team'
export type Feature = typeof allFeatures[0]

// Helper functions
export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'selfhosted': return 'Self-hosted'
    case 'personal': return 'Personal'
    case 'team': return 'Team'
    default: return tier
  }
}

export function getTierDescription(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'selfhosted': return 'Run on your own infrastructure'
    case 'personal': return 'For individual Bitcoin holders'
    case 'team': return 'For Uncle Jims & family guardians'
    default: return ''
  }
}

