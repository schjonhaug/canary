import { render, screen } from '@testing-library/react'
import CloudPageContent from '../cloud-page'
import type { PricingData } from '@/hooks/usePricing'

const testNonce = 'cloud-content-test-nonce'

const samplePricing: PricingData = {
  tiers: [
    {
      tier: 'team',
      name: 'Team',
      monthly_price: { price_id: 'team-month', amount: 2900, currency: 'usd', interval: 'month' },
      yearly_price: { price_id: 'team-year', amount: 27840, currency: 'usd', interval: 'year' },
      features: {},
    },
    {
      tier: 'personal',
      name: 'Personal',
      monthly_price: { price_id: 'personal-month', amount: 900, currency: 'usd', interval: 'month' },
      yearly_price: { price_id: 'personal-year', amount: 8640, currency: 'usd', interval: 'year' },
      features: {},
    },
  ],
  yearly_discount_percent: 20,
}

// The plan cards fetch prices themselves; the mock hands the page whatever this holds.
const mockPricingState: { pricing: PricingData | null } = { pricing: samplePricing }
const mockPlanComparison = jest.fn()

jest.mock('../plan-comparison', () => {
  const { useEffect } = jest.requireActual<typeof import('react')>('react')
  const PlanComparison = (props: { onPricingLoaded?: (pricing: PricingData) => void }) => {
    mockPlanComparison(props)
    useEffect(() => {
      if (mockPricingState.pricing) props.onPricingLoaded?.(mockPricingState.pricing)
    }, [props.onPricingLoaded])
    return <div data-testid="plan-comparison">Plan comparison</div>
  }
  return { PlanComparison }
})

function structuredData() {
  return Array.from(document.querySelectorAll('script[type="application/ld+json"]')).map((script) => ({
    nonce: script.getAttribute('nonce'),
    data: JSON.parse(script.textContent ?? ''),
  }))
}

describe('CloudPageContent', () => {
  beforeEach(() => {
    mockPlanComparison.mockClear()
    mockPricingState.pricing = samplePricing
  })

  it('places pricing before the privacy FAQ', () => {
    render(<CloudPageContent nonce={testNonce} />)

    const pricing = screen.getByRole('heading', { name: 'Canary Cloud plans' })
    const faq = screen.getByRole('heading', { name: 'Privacy questions' })

    expect(pricing.compareDocumentPosition(faq) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByText('Canary Cloud stores the descriptors, XPUBs, or addresses you supply, including single-sig and multisig wallets.')).toBeInTheDocument()
    expect(screen.getByText('Yes. That watch-only information can reveal wallet addresses, balances, and transaction history.')).toBeInTheDocument()
    expect(screen.getByText('Your Canary account and subscription can connect wallet information to your account and billing identity.')).toBeInTheDocument()
    expect(screen.getByText('No. Canary Cloud never receives private keys and cannot sign transactions or spend your funds.')).toBeInTheDocument()
  })

  it('integrates the existing pricing comparison and signup flow', () => {
    render(<CloudPageContent nonce={testNonce} />)

    expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    expect(mockPlanComparison).toHaveBeenCalledWith(expect.objectContaining({
      showPricing: true,
      showCallToAction: true,
      showUnifiedTrialButton: true,
    }))
    expect(screen.getByText('Email notifications')).toBeInTheDocument()
    expect(screen.getByText('SMS notifications')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Start free trial/ })).toHaveAttribute('href', '/sign-up')
    expect(screen.getByRole('link', { name: /Sign up/ })).toHaveAttribute('href', '/sign-up')
  })

  it('links to self hosting, demo, sign in, and GitHub', () => {
    render(<CloudPageContent nonce={testNonce} />)

    expect(screen.getAllByRole('link', { name: /self host/i })[0]).toHaveAttribute('href', '/#install')
    expect(screen.getAllByRole('link', { name: /demo/i })[0]).toHaveAttribute('href', '/demo')
    expect(screen.getAllByRole('link', { name: /sign in/i })[0]).toHaveAttribute('href', '/sign-in')
    expect(screen.getAllByRole('link', { name: /GitHub/i })[0]).toHaveAttribute('target', '_blank')
  })

  it('answers what Canary Cloud is, who it is for, how it works and what it costs', () => {
    render(<CloudPageContent nonce={testNonce} />)

    expect(screen.getByRole('heading', { level: 2, name: 'What is Canary Cloud?' })).toBeInTheDocument()
    expect(screen.getByText(/Canary Cloud is the hosted version of Canary Wallet/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 3, name: 'Who is it for?' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 3, name: 'How does it work?' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 3, name: 'What does it cost?' })).toBeInTheDocument()
    expect(screen.getByText(/two plans, Personal and Team/)).toBeInTheDocument()

    const planCosts = screen.getAllByRole('listitem').map((item) => item.textContent)
    expect(planCosts).toContain('Personal (For individual Bitcoin holders): $9 a month or $86.40 a year. 1 wallet, 1 contact per wallet, 10-minute sync.')
    expect(planCosts).toContain('Team (For Uncle Jims & family guardians): $29 a month or $278.40 a year. 5 wallets, 5 contacts per wallet, 2-minute sync.')
    expect(planCosts.indexOf(planCosts.find((text) => text?.startsWith('Personal')) ?? ''))
      .toBeLessThan(planCosts.indexOf(planCosts.find((text) => text?.startsWith('Team')) ?? ''))
  })

  it('mirrors the visible privacy questions in FAQPage structured data with the request nonce', () => {
    render(<CloudPageContent nonce={testNonce} />)

    const faq = structuredData().find(({ data }) => data['@type'] === 'FAQPage')
    expect(faq?.nonce).toBe(testNonce)
    expect(faq?.data.mainEntity).toHaveLength(5)

    for (const question of faq?.data.mainEntity ?? []) {
      expect(question['@type']).toBe('Question')
      expect(screen.getByText(question.name)).toBeInTheDocument()
      expect(screen.getByText(question.acceptedAnswer.text)).toBeInTheDocument()
    }
    expect(faq?.data.mainEntity.map((question: { name: string }) => question.name)).toContain('Does self-hosting avoid these tradeoffs?')
  })

  it('describes Canary Cloud as a Product with one Offer per fetched price', () => {
    render(<CloudPageContent nonce={testNonce} />)

    const product = structuredData().find(({ data }) => data['@type'] === 'Product')
    expect(product?.nonce).toBe(testNonce)
    expect(product?.data).toEqual(expect.objectContaining({
      name: 'Canary Cloud',
      url: 'https://canarybitcoin.com/cloud',
      brand: { '@type': 'Brand', name: 'Canary Wallet' },
    }))
    expect(screen.getByText(product?.data.description)).toBeInTheDocument()

    expect(product?.data.offers.map((offer: { name: string; price: string; priceCurrency: string }) => [offer.name, offer.price, offer.priceCurrency])).toEqual([
      ['Personal, billed monthly', '9.00', 'USD'],
      ['Personal, billed yearly', '86.40', 'USD'],
      ['Team, billed monthly', '29.00', 'USD'],
      ['Team, billed yearly', '278.40', 'USD'],
    ])
    expect(product?.data.offers[1].priceSpecification).toEqual(expect.objectContaining({ billingDuration: 1, unitCode: 'ANN' }))
  })

  it('leaves prices out until the plan cards have fetched them', () => {
    mockPricingState.pricing = null
    render(<CloudPageContent nonce={testNonce} />)

    expect(screen.queryByText(/a month or/)).not.toBeInTheDocument()
    const product = structuredData().find(({ data }) => data['@type'] === 'Product')
    expect(product?.data).not.toHaveProperty('offers')
    expect(JSON.stringify(product?.data)).not.toContain('$')
  })
})
