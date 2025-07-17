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
export function formatBitcoinAmount(sats: number | null | undefined): string {
  if (sats === null || sats === undefined) return "0.00000000 BTC"
  const btc = sats / 100_000_000
  return `${btc.toLocaleString(undefined, { 
    minimumFractionDigits: 8, 
    maximumFractionDigits: 8 
  })} BTC`
}

// Date formatting utilities
export function formatDateTime(dateTime: string): string {
  const date = new Date(dateTime)
  return date.toLocaleString()
}

export function formatDate(dateTime: string): string {
  const date = new Date(dateTime)
  return date.toLocaleDateString()
}

// API utilities
export function getApiBaseUrl(): string {
  return process.env.NEXT_PUBLIC_API_URL || ''
}

export async function handleApiResponse(response: Response): Promise<unknown> {
  if (!response.ok) {
    if (response.status === 404) {
      throw new Error('Resource not found')
    }
    if (response.status === 409) {
      throw new Error('Resource already exists')
    }
    
    // Try to get error message from response
    try {
      const errorData = await response.json()
      throw new Error(errorData.error || `HTTP error! status: ${response.status}`)
    } catch {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
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
