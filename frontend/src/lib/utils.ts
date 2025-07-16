import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function extractChecksum(descriptor: string): string {
  const checksumMatch = descriptor.match(/#([a-zA-Z0-9]+)$/)
  return checksumMatch ? checksumMatch[1] : "Unknown"
}

export function checksumToHexColor(checksum: string): string {
  // DJB2 hash algorithm with position weighting for better distribution
  let hash = 5381
  for (let i = 0; i < checksum.length; i++) {
    const char = checksum.charCodeAt(i)
    // DJB2: hash = ((hash << 5) + hash) + char
    // Add position weighting to further improve distribution
    hash = ((hash << 5) + hash) + char * (i + 1)
  }
  
  // Ensure positive number and get hue (0-360 degrees)
  const hue = Math.abs(hash) % 360
  
  // Fixed saturation and lightness for consistent appearance
  const saturation = 70 // 70% saturation for vibrant colors
  const lightness = 50  // 50% lightness for good contrast
  
  // Convert HSL to RGB
  const c = (1 - Math.abs(2 * (lightness / 100) - 1)) * (saturation / 100)
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1))
  const m = (lightness / 100) - c / 2
  
  let r, g, b
  if (hue < 60) { r = c; g = x; b = 0 }
  else if (hue < 120) { r = x; g = c; b = 0 }
  else if (hue < 180) { r = 0; g = c; b = x }
  else if (hue < 240) { r = 0; g = x; b = c }
  else if (hue < 300) { r = x; g = 0; b = c }
  else { r = c; g = 0; b = x }
  
  r = Math.round((r + m) * 255)
  g = Math.round((g + m) * 255)
  b = Math.round((b + m) * 255)
  
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`
}

// Cache for the SVG content to avoid re-fetching
let cachedSvgContent: string | null = null

export async function loadCanarySvg(checksum: string): Promise<string> {
  const calculatedColor = checksumToHexColor(checksum)
  
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
      return '<div>⚠️</div>' // Fallback if SVG fails to load
    }
  }
  
  // Replace colors in the SVG content
  return cachedSvgContent
    .replace(/#F6C919/g, calculatedColor)  // Replace yellow with calculated color
    .replace(/#73C2DE/g, 'transparent')    // Make blue transparent
    .replace(/width="691"/g, 'width="24"')  // Resize to 24x24
    .replace(/height="595"/g, 'height="24"')
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
