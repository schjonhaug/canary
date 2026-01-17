import { useState, useEffect } from 'react'
import { api } from '@/lib/api'

export interface StripePricingTier {
  tier: string
  name: string
  description?: string
  monthly_price?: {
    price_id: string
    amount: number  // in cents
    currency: string
    interval: string
  }
  yearly_price?: {
    price_id: string
    amount: number  // in cents
    currency: string
    interval: string
  }
  features: Record<string, string>
}

export interface PricingData {
  tiers: StripePricingTier[]
  yearly_discount_percent?: number
}

export function usePricing() {
  const [pricing, setPricing] = useState<PricingData | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const fetchPricing = async () => {
      try {
        setLoading(true)
        setError(null)
        const data = await api.getBillingPricing()
        setPricing(data)
      } catch (err) {
        console.error('Failed to fetch pricing:', err)
        setError(err instanceof Error ? err.message : 'Failed to load pricing')
      } finally {
        setLoading(false)
      }
    }

    fetchPricing()
  }, [])

  return {
    pricing,
    loading,
    error,
    refetch: async () => {
      setLoading(true)
      try {
        const data = await api.getBillingPricing()
        setPricing(data)
        setError(null)
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load pricing')
      } finally {
        setLoading(false)
      }
    }
  }
}

// Helper function to format price from cents to dollars
export function formatPrice(amountInCents: number, currency: string, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: 'currency',
    currency: currency.toUpperCase(),
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(amountInCents / 100)
}

// Helper function to calculate yearly discount percentage
export function calculateYearlyDiscount(monthlyAmount: number, yearlyAmount: number): number {
  const fullYearlyPrice = monthlyAmount * 12
  const discountAmount = fullYearlyPrice - yearlyAmount
  return Math.round((discountAmount / fullYearlyPrice) * 100)
}

// Helper function to get tier display order (personal -> team)
export function getTierOrder(tier: string): number {
  switch (tier.toLowerCase()) {
    case 'personal': return 1
    case 'team': return 2
    default: return 99
  }
}

// Helper function to sort tiers in display order
export function sortTiers(tiers: StripePricingTier[]): StripePricingTier[] {
  return [...tiers].sort((a, b) => getTierOrder(a.tier) - getTierOrder(b.tier))
}