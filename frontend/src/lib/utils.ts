import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

// =============================================================================
// API Error Types and Classes
// =============================================================================

/**
 * Error types for API responses
 *
 * - network: Network connectivity issues (fetch failed, timeout, etc.)
 * - validation: Input validation errors (400 Bad Request)
 * - authentication: Auth errors (401 Unauthorized)
 * - forbidden: Permission errors (403 Forbidden)
 * - not_found: Resource not found (404 Not Found)
 * - conflict: Resource conflict (409 Conflict)
 * - server: Server-side errors (500+)
 * - unknown: Unclassified errors
 */
export type ApiErrorType =
  | 'network'
  | 'validation'
  | 'authentication'
  | 'forbidden'
  | 'not_found'
  | 'conflict'
  | 'server'
  | 'unknown'

/**
 * Typed API error class for structured error handling
 *
 * Provides:
 * - Error type categorization for UI differentiation
 * - HTTP status code for debugging
 * - User-friendly error message
 * - Helper methods for error type checking
 */
export class ApiError extends Error {
  public readonly type: ApiErrorType
  public readonly statusCode: number | null

  constructor(message: string, type: ApiErrorType, statusCode: number | null = null) {
    super(message)
    this.name = 'ApiError'
    this.type = type
    this.statusCode = statusCode

    // Maintains proper stack trace for where our error was thrown (only in V8)
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, ApiError)
    }
  }

  /**
   * Check if this is a network-related error (connectivity issues)
   */
  isNetworkError(): boolean {
    return this.type === 'network'
  }

  /**
   * Check if this is a validation error (user input issues)
   */
  isValidationError(): boolean {
    return this.type === 'validation'
  }

  /**
   * Check if this is an authentication error
   */
  isAuthError(): boolean {
    return this.type === 'authentication' || this.type === 'forbidden'
  }

  /**
   * Check if this is a server error (backend issues)
   */
  isServerError(): boolean {
    return this.type === 'server'
  }

  /**
   * Get a user-friendly error message based on error type
   */
  getUserFriendlyMessage(): string {
    switch (this.type) {
      case 'network':
        return 'Unable to connect. Please check your internet connection and try again.'
      case 'authentication':
        return 'Please sign in to continue.'
      case 'forbidden':
        return 'You do not have permission to perform this action.'
      case 'server':
        return 'Something went wrong on our end. Please try again later.'
      default:
        return this.message
    }
  }
}

/**
 * Determine the error type from HTTP status code
 */
function getErrorTypeFromStatus(status: number): ApiErrorType {
  if (status === 400) return 'validation'
  if (status === 401) return 'authentication'
  if (status === 403) return 'forbidden'
  if (status === 404) return 'not_found'
  if (status === 409) return 'conflict'
  if (status >= 500) return 'server'
  return 'unknown'
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

// Bitcoin amount formatting for balances (no trailing zeros)
export function formatBitcoinAmount(sats: number | null | undefined): string {
  if (sats === null || sats === undefined) return "0 BTC"

  const btc = sats / 100_000_000
  const formattedAmount = btc.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 8
  })

  return `${formattedAmount} BTC`
}

// Bitcoin amount formatting for transactions (full 8-digit precision)
export function formatTransactionAmount(sats: number | null | undefined, eventType?: 'send' | 'receive'): string {
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
    const errorType = getErrorTypeFromStatus(response.status)
    let errorMessage: string

    // Try to get error message from response first
    try {
      const errorData = await response.json()
      errorMessage = errorData.error || errorData.message || getDefaultErrorMessage(response.status)
    } catch {
      errorMessage = getDefaultErrorMessage(response.status)
    }

    throw new ApiError(errorMessage, errorType, response.status)
  }

  // Return JSON if response has content
  const contentType = response.headers.get('content-type')
  if (contentType && contentType.includes('application/json')) {
    return response.json()
  }

  return null
}

/**
 * Get default error message based on HTTP status code
 */
function getDefaultErrorMessage(status: number): string {
  switch (status) {
    case 400:
      return 'Invalid request. Please check your input.'
    case 401:
      return 'Please sign in to continue.'
    case 403:
      return 'You do not have permission to perform this action.'
    case 404:
      return 'Resource not found'
    case 409:
      return 'Resource already exists'
    case 422:
      return 'Unable to process request. Please check your input.'
    case 429:
      return 'Too many requests. Please wait a moment and try again.'
    case 500:
    case 502:
    case 503:
    case 504:
      return 'Something went wrong on our end. Please try again later.'
    default:
      return `HTTP error! status: ${status}`
  }
}

/**
 * Create a network error (for use when fetch fails entirely)
 */
export function createNetworkError(originalError?: Error): ApiError {
  const message = originalError?.message || 'Network request failed'
  return new ApiError(
    message.includes('fetch') || message.includes('network')
      ? 'Unable to connect. Please check your internet connection and try again.'
      : message,
    'network',
    null
  )
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
    minimumFractionDigits: 0,
    maximumFractionDigits: 8
  })
}

export function parseBtcInput(input: string): number | null {
  const trimmed = input.trim()
  if (!trimmed) return null

  // Use the browser's locale to determine decimal separator
  const formatter = new Intl.NumberFormat()
  const parts = formatter.formatToParts(1.1)
  const decimalSeparator = parts.find(part => part.type === 'decimal')?.value || '.'

  // Normalize the input based on the browser's locale
  let normalizedInput = trimmed
  if (decimalSeparator === ',') {
    // If locale uses comma as decimal, replace comma with dot for parseFloat
    normalizedInput = trimmed.replace(',', '.')
  }

  const num = parseFloat(normalizedInput)
  if (isNaN(num)) return null

  return num
}

export function getBtcPlaceholder(): string {
  // Use the browser's locale to determine decimal separator
  const formatter = new Intl.NumberFormat()
  const parts = formatter.formatToParts(1.1)
  const decimalSeparator = parts.find(part => part.type === 'decimal')?.value || '.'

  // Return placeholder with locale-appropriate decimal separator
  return `0${decimalSeparator}00000000`
}
