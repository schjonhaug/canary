import type { PricingData } from '@/hooks/usePricing'

const PRICING_TIMEOUT_MS = 3000
const PRICING_REVALIDATE_SECONDS = 3600

// Fetches the Stripe prices on the server, so a page can put them in the HTML
// (structured data offers) instead of waiting for the browser. Returns null on
// any failure; callers then render as they would before the prices arrive.
export async function fetchPricingOnServer(): Promise<PricingData | null> {
  const apiUrl = (process.env.API_URL || process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000').replace(/\/+$/, '')
  try {
    const response = await fetch(`${apiUrl}/api/billing/pricing`, {
      signal: AbortSignal.timeout(PRICING_TIMEOUT_MS),
      next: { revalidate: PRICING_REVALIDATE_SECONDS },
    })
    if (!response.ok) return null
    const data = (await response.json()) as PricingData
    return Array.isArray(data?.tiers) ? data : null
  } catch {
    return null
  }
}
