import React from 'react'
import { render, screen } from '@testing-library/react'
import SubscriptionPage from '../page'

const mockGetSearchParam = jest.fn()
const mockRefreshBillingStatus = jest.fn()
const mockUseAuth = jest.fn()

jest.mock('next/navigation', () => ({
  useSearchParams: () => ({
    get: mockGetSearchParam,
  }),
}))

jest.mock('../../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../../../lib/api', () => ({
  api: {
    createCustomerPortalSession: jest.fn(),
  },
}))

jest.mock('../../../components/plans-modal', () => ({
  PlansModal: () => <div data-testid="plans-modal" />,
}))

describe('SubscriptionPage', () => {
  const billingStatus = {
    user_id: 'user-1',
    subscription_tier: 'team',
    subscription_status: 'active',
    stripe_customer_id: 'cus_123',
    wallet_count: 2,
    contact_count: 6,
    limits: {
      max_wallets: 5,
      max_contacts_per_wallet: 5,
      sync_interval_seconds: 120,
    },
  }

  beforeEach(() => {
    mockGetSearchParam.mockReturnValue(null)
    mockRefreshBillingStatus.mockClear()
    mockUseAuth.mockReturnValue({
      user: {
        id: 1,
        email: 'test@example.com',
        subscription_tier: 'team',
      },
      billingStatus,
      isLoading: false,
      refreshBillingStatus: mockRefreshBillingStatus,
      isSelfHostedMode: false,
    })
  })

  it('shows total contacts separately from the per-wallet contact limit', () => {
    render(<SubscriptionPage />)

    expect(screen.getByText('6 total across wallets')).toBeInTheDocument()
    expect(screen.getByText('Limit: 5 per wallet')).toBeInTheDocument()
    expect(screen.queryByText('6 / 5')).not.toBeInTheDocument()
  })

  it('highlights contacts only when total contacts exceed total possible capacity', () => {
    mockUseAuth.mockReturnValue({
      user: {
        id: 1,
        email: 'test@example.com',
        subscription_tier: 'personal',
      },
      billingStatus: {
        ...billingStatus,
        subscription_tier: 'personal',
        wallet_count: 1,
        contact_count: 2,
        limits: {
          ...billingStatus.limits,
          max_wallets: 1,
          max_contacts_per_wallet: 1,
        },
      },
      isLoading: false,
      refreshBillingStatus: mockRefreshBillingStatus,
      isSelfHostedMode: false,
    })

    render(<SubscriptionPage />)

    expect(screen.getByText('2 total across wallets', { exact: false })).toHaveClass('text-orange-600')
    expect(screen.getByText('(over limit)')).toBeInTheDocument()
  })

  it('highlights inconsistent zero-capacity contact usage', () => {
    mockUseAuth.mockReturnValue({
      user: {
        id: 1,
        email: 'test@example.com',
        subscription_tier: 'personal',
      },
      billingStatus: {
        ...billingStatus,
        subscription_tier: 'personal',
        wallet_count: 0,
        contact_count: 1,
        limits: {
          ...billingStatus.limits,
          max_wallets: 1,
          max_contacts_per_wallet: 1,
        },
      },
      isLoading: false,
      refreshBillingStatus: mockRefreshBillingStatus,
      isSelfHostedMode: false,
    })

    render(<SubscriptionPage />)

    expect(screen.getByText('1 total across wallets')).toHaveClass('text-orange-600')
  })

  it('does not warn when the per-wallet contact limit is unavailable', () => {
    mockUseAuth.mockReturnValue({
      user: {
        id: 1,
        email: 'test@example.com',
        subscription_tier: 'team',
      },
      billingStatus: {
        ...billingStatus,
        contact_count: 6,
        limits: {
          max_wallets: 5,
          sync_interval_seconds: 120,
        },
      },
      isLoading: false,
      refreshBillingStatus: mockRefreshBillingStatus,
      isSelfHostedMode: false,
    })

    render(<SubscriptionPage />)

    expect(screen.getByText('6 total across wallets')).not.toHaveClass('text-orange-600')
    expect(screen.queryByText(/Limit:/)).not.toBeInTheDocument()
  })
})
