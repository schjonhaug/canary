import React from 'react'
import { render, screen } from '@testing-library/react'
import WalletsPage from '../page'
import type { Wallet } from '../../../types'

const mockPush = jest.fn()
const mockUseAuth = jest.fn()
const mockUseWalletsContext = jest.fn()

jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('../../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../../../contexts/wallets-context', () => ({
  useWalletsContext: () => mockUseWalletsContext(),
}))

jest.mock('../../../components/wallet-cards', () => ({
  WalletCards: () => <div data-testid="wallet-cards" />,
}))

jest.mock('../../../components/wallet-onboarding', () => ({
  WalletOnboarding: () => <div data-testid="wallet-onboarding" />,
}))

jest.mock('../../../components/ui/loading-spinner', () => ({
  LoadingSpinner: () => <div data-testid="loading-spinner" />,
}))

jest.mock('../../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatBitcoinAmount: (value: number) => `${value} sats`,
    formatFiatAmount: (value: number, currency: string) => `${value} ${currency}`,
    locale: 'en-US',
  }),
}))

describe('WalletsPage', () => {
  const wallets: Wallet[] = [
    {
      checksum: 'wallet-1',
      name: 'Wallet 1',
      descriptor: 'wpkh([abcd/84h/0h/0h]xpub/0/*)',
      wallet_filename: 'wallet-1',
      hex_color: '#f59e0b',
      created_at: '2024-01-01T00:00:00Z',
      balance_total: 1000,
      last_activity: null,
      status: 'ready',
      contact_count: 1,
      is_active: true,
      wallet_type: 'descriptor',
    },
    {
      checksum: 'wallet-2',
      name: 'Wallet 2',
      descriptor: 'wpkh([abcd/84h/0h/0h]xpub/0/*)',
      wallet_filename: 'wallet-2',
      hex_color: '#f59e0b',
      created_at: '2024-01-02T00:00:00Z',
      balance_total: 2000,
      last_activity: null,
      status: 'ready',
      contact_count: 1,
      is_active: true,
      wallet_type: 'descriptor',
    },
  ]

  beforeEach(() => {
    mockPush.mockClear()
    mockUseWalletsContext.mockReturnValue({
      wallets,
      error: null,
      lastUpdate: null,
      isConnected: true,
      isLoading: false,
    })
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: true,
      billingStatus: {
        subscription_status: 'active',
        limits: {
          max_wallets: 5,
        },
      },
    })
  })

  it('shows wallet usage when cloud wallet limits are finite', () => {
    render(<WalletsPage />)

    expect(screen.getByText('2 / 5')).toBeInTheDocument()
    expect(screen.getByLabelText('2 of 5 wallets used')).toBeInTheDocument()
  })

  it('hides wallet usage when limits are disabled', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: false,
      billingStatus: {
        subscription_status: 'active',
        limits: {
          max_wallets: -1,
        },
      },
    })

    render(<WalletsPage />)

    expect(screen.queryByText('2 / -1')).not.toBeInTheDocument()
    expect(screen.queryByLabelText(/wallets used/)).not.toBeInTheDocument()
  })

  it('hides wallet usage when billing limits are unavailable', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: true,
      billingStatus: {
        subscription_status: 'active',
      },
    })

    render(<WalletsPage />)

    expect(screen.queryByLabelText(/wallets used/)).not.toBeInTheDocument()
  })

  it('hides wallet usage while billing status is unavailable', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: true,
      billingStatus: null,
    })

    render(<WalletsPage />)

    expect(screen.queryByLabelText(/wallets used/)).not.toBeInTheDocument()
  })

  it('does not highlight wallet usage at the limit', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: true,
      billingStatus: {
        subscription_status: 'active',
        limits: {
          max_wallets: 2,
        },
      },
    })

    render(<WalletsPage />)

    expect(screen.getByText('2 / 2')).not.toHaveClass('text-orange-700')
  })

  it('highlights wallet usage when over the limit', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      user: { id: 1, email: 'test@example.com' },
      isCloudMode: true,
      billingStatus: {
        subscription_status: 'active',
        limits: {
          max_wallets: 1,
        },
      },
    })

    render(<WalletsPage />)

    expect(screen.getByText('2 / 1')).toHaveClass('text-orange-700')
  })
})
