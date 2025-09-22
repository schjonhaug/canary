import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}


// Cache for the SVG content to avoid re-fetching
let cachedSvgContent: string | null = null

// Cache for processed SVGs by color to avoid re-processing
const processedSvgCache = new Map<string, string>()

// Synchronous function to get cached SVG if available
export function getCachedCanarySvg(hexColor: string): string | null {
  return processedSvgCache.get(hexColor) || null
}

export async function loadCanarySvg(hexColor: string): Promise<string> {
  // Return cached processed SVG if available
  if (processedSvgCache.has(hexColor)) {
    return processedSvgCache.get(hexColor)!
  }
  
  // Load SVG content if not cached
  if (!cachedSvgContent) {
    try {
      const response = await fetch('/images/canary.svg')
      if (!response.ok) {
        throw new Error('Failed to load SVG')
      }
      cachedSvgContent = await response.text()
    } catch (error) {
      console.error('Error loading canary SVG:', error)
      const fallback = '<div>⚠️</div>' // Fallback if SVG fails to load
      processedSvgCache.set(hexColor, fallback)
      return fallback
    }
  }
  
  // Replace colors in the SVG content
  const processedSvg = cachedSvgContent
    .replace(/#F6C919/g, hexColor)  // Replace yellow with provided hex color
    .replace(/#73C2DE/g, 'transparent')    // Make blue transparent
    .replace(/width="691"/g, 'width="24"')  // Resize to 24x24
    .replace(/height="595"/g, 'height="24"')
  
  // Cache the processed SVG
  processedSvgCache.set(hexColor, processedSvg)
  
  return processedSvg
}

// Bitcoin amount formatting utility  
export function formatBitcoinAmount(sats: number | null | undefined, eventType?: 'send' | 'receive'): string {
  if (sats === null || sats === undefined) return "0.00000000 BTC"
  
  const absSats = Math.abs(sats)
  const btc = absSats / 100_000_000
  const formattedAmount = btc.toLocaleString(undefined, { 
    minimumFractionDigits: 8, 
    maximumFractionDigits: 8 
  })
  
  // Show sign based on event type for better UX
  if (eventType === 'send') {
    return `−${formattedAmount} BTC`  // Unicode minus sign
  } else if (eventType === 'receive') {
    return `+${formattedAmount} BTC`
  }
  
  // Default: no sign (for balance totals, etc.)
  return `${formattedAmount} BTC`
}

// Date formatting utilities
export function formatDateTime(dateTime: string | number): string {
  let date: Date
  
  if (typeof dateTime === 'number') {
    // Convert Unix timestamp to milliseconds
    date = new Date(dateTime * 1000)
  } else {
    // SQLite timestamps are in UTC but without timezone indicator
    // Need to explicitly treat them as UTC
    if (dateTime.match(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)) {
      // Format: "2025-09-03 13:26:11" - treat as UTC
      date = new Date(dateTime + ' UTC')
    } else {
      date = new Date(dateTime)
    }
  }
  
  // Ensure valid date object
  if (isNaN(date.getTime())) {
    return "Invalid date"
  }
  
  // Use browser's locale with consistent formatting options
  return date.toLocaleString(undefined, {
    year: '2-digit',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false // Use 24-hour format for consistency
  })
}

export function formatDate(dateTime: string): string {
  const date = new Date(dateTime)
  return date.toLocaleDateString()
}

// API utilities
export function getApiBaseUrl(): string {
  // Only use NEXT_PUBLIC_API_URL if it's actually set to a non-empty value
  // This allows the Next.js proxy to work when the env var is not set
  return process.env.NEXT_PUBLIC_API_URL && process.env.NEXT_PUBLIC_API_URL.trim() !== '' 
    ? process.env.NEXT_PUBLIC_API_URL 
    : ''
}

export async function handleApiResponse(response: Response): Promise<unknown> {
  if (!response.ok) {
    // Try to get error message from response first
    let errorMessage: string
    try {
      const errorData = await response.json()
      errorMessage = errorData.error || errorData.message || `HTTP error! status: ${response.status}`
    } catch {
      // Fallback to generic messages based on status code
      if (response.status === 404) {
        errorMessage = 'Resource not found'
      } else if (response.status === 409) {
        errorMessage = 'Resource already exists'
      } else {
        errorMessage = `HTTP error! status: ${response.status}`
      }
    }
    
    throw new Error(errorMessage)
  }
  
  // Return JSON if response has content
  const contentType = response.headers.get('content-type')
  if (contentType && contentType.includes('application/json')) {
    return response.json()
  }
  
  return null
}

// Common error styles
export const errorStyles = "p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700"
export const successStyles = "p-3 bg-green-50 border border-green-200 rounded-lg text-sm text-green-700"

// Subscription tier utilities
export const SUBSCRIPTION_LIMITS = {
  personal: { wallets: 1, contacts: 1 },
  team: { wallets: 5, contacts: 5 },
} as const

export type SubscriptionTier = keyof typeof SUBSCRIPTION_LIMITS

export function getWalletLimit(tier: string): number {
  const tierData = SUBSCRIPTION_LIMITS[tier.toLowerCase() as SubscriptionTier]
  return tierData ? tierData.wallets : 1 // Default to personal limit for unknown tiers
}

export function hasReachedWalletLimit(currentWalletCount: number, tier: string): boolean {
  const limit = getWalletLimit(tier)
  return currentWalletCount >= limit
}

export function getContactLimit(tier: string): number {
  const tierData = SUBSCRIPTION_LIMITS[tier.toLowerCase() as SubscriptionTier]
  return tierData ? tierData.contacts : 1 // Default to personal limit for unknown tiers
}

export function hasReachedContactLimit(currentContactCount: number, tier: string): boolean {
  const limit = getContactLimit(tier)
  return currentContactCount >= limit
}


export function getTierDisplayName(tier: string): string {
  switch (tier.toLowerCase()) {
    case 'personal':
      return 'Personal'
    case 'team':
      return 'Team'
    default:
      return tier
  }
}

// Bitcoin amount input utilities
export function satsToBtc(sats: number): number {
  return sats / 100_000_000
}

export function btcToSats(btc: number): number {
  return Math.round(btc * 100_000_000)
}

export function formatBtcAmount(btc: number): string {
  return btc.toLocaleString(undefined, {
    minimumFractionDigits: 8,
    maximumFractionDigits: 8
  })
}

export function parseBtcInput(input: string): number | null {
  const trimmed = input.trim()
  if (!trimmed) return null

  const num = parseFloat(trimmed)
  if (isNaN(num) || num < 0) return null

  return num
}
