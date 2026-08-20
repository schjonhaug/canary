import React from 'react'
import { act, render, screen, waitFor } from '@testing-library/react'
import BillingSuccessPage, {
  BILLING_FAST_POLL_DELAY_MS,
  BILLING_FAST_POLL_LIMIT,
  BILLING_SLOW_POLL_DELAY_MS,
  billingPollDelay,
} from '../success'

const mockGetCheckoutSessionDetails = jest.fn()
const mockRefreshBillingStatus = jest.fn().mockResolvedValue(undefined)

jest.mock('next/navigation', () => ({
  useSearchParams: () => new URLSearchParams('session=checkout-token'),
}))

jest.mock('@/lib/api', () => ({
  api: {
    getCheckoutSessionDetails: (...args: unknown[]) => mockGetCheckoutSessionDetails(...args),
  },
}))

jest.mock('@/contexts/auth-context', () => ({
  useAuth: () => ({ refreshBillingStatus: mockRefreshBillingStatus }),
}))

jest.mock('@/hooks/usePricing', () => ({
  usePricing: () => ({ pricing: { yearly_discount_percent: 20 } }),
  formatPrice: (amount: number) => String(amount),
}))

describe('BillingSuccessPage polling', () => {
  beforeEach(() => {
    jest.useFakeTimers()
    jest.clearAllMocks()
  })

  afterEach(() => {
    jest.useRealTimers()
  })

  it('backs off instead of stopping after the fast polling window', () => {
    expect(billingPollDelay(BILLING_FAST_POLL_LIMIT)).toBe(BILLING_FAST_POLL_DELAY_MS)
    expect(billingPollDelay(BILLING_FAST_POLL_LIMIT + 1)).toBe(BILLING_SLOW_POLL_DELAY_MS)
  })

  it('retries a transient error after session details have loaded', async () => {
    mockGetCheckoutSessionDetails
      .mockResolvedValueOnce({ status: 'pending', tier: 'personal', billing_period: 'monthly' })
      .mockRejectedValueOnce(new Error('temporary network error'))
      .mockResolvedValueOnce({ status: 'complete', tier: 'personal', billing_period: 'monthly' })

    render(<BillingSuccessPage />)
    await waitFor(() => expect(mockGetCheckoutSessionDetails).toHaveBeenCalledTimes(1))
    expect(await screen.findByText('Payment Processing')).toBeInTheDocument()

    await act(async () => {
      jest.advanceTimersByTime(BILLING_FAST_POLL_DELAY_MS)
    })
    expect(mockGetCheckoutSessionDetails).toHaveBeenCalledTimes(2)
    expect(screen.queryByText('Unable to Load Session')).not.toBeInTheDocument()

    await act(async () => {
      jest.advanceTimersByTime(BILLING_SLOW_POLL_DELAY_MS)
    })
    await waitFor(() => expect(mockGetCheckoutSessionDetails).toHaveBeenCalledTimes(3))
    expect(await screen.findByText('Payment Successful!')).toBeInTheDocument()
    expect(mockRefreshBillingStatus).toHaveBeenCalledTimes(1)
  })
})
