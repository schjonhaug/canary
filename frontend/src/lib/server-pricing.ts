import type { PricingData } from '@/hooks/usePricing'

const PRICING_TIMEOUT_MS = 3000
const PRICING_REVALIDATE_SECONDS = 3600

// Fetches Stripe prices on the server, so the first HTML (what crawlers read) already
// carries them. Returns null on any failure; the plan cards still fetch in the browser.
export async function fetchServerPricing(): Promise<PricingData | null> {
  const apiUrl = process.env.API_URL || process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000'
  try {
    const response = await fetch(`${apiUrl}/api/billing/pricing`, {
      next: { revalidate: PRICING_REVALIDATE_SECONDS },
      signal: AbortSignal.timeout(PRICING_TIMEOUT_MS),
    })
    if (!response.ok) return null
    const data = (await response.json()) as PricingData
    return Array.isArray(data?.tiers) ? data : null
  } catch {
    return null
  }
}
