import React from 'react'
import { render, screen } from '@testing-library/react'
import { WalletInfoSidebar } from '../wallet-detail/wallet-info-sidebar'
import type { Contact, Wallet } from '../../types'

const mockUseAuth = jest.fn()

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatBitcoinAmount: (value: number) => `${value} sats`,
    formatFiatAmount: (value: number, currency: string) => `${value} ${currency}`,
  }),
}))

jest.mock('../wallet-contacts-list', () => ({
  WalletContactsList: () => <div data-testid="wallet-contacts-list" />,
}))

jest.mock('../balance-alerts-list', () => ({
  BalanceAlertsList: () => <div data-testid="balance-alerts-list" />,
}))

jest.mock('../wallet-detail/wallet-details-section', () => ({
  WalletDetailsSection: () => <div data-testid="wallet-details-section" />,
}))

describe('WalletInfoSidebar', () => {
  const wallet: Wallet = {
    checksum: 'test-checksum',
    name: 'Test Wallet',
    descriptor: 'wpkh([abcd/84h/0h/0h]xpub/0/*)',
    wallet_filename: 'test-wallet',
    hex_color: '#f59e0b',
    created_at: '2024-01-01T00:00:00Z',
    balance_total: 1000,
    last_activity: null,
    status: 'ready',
    contact_count: 2,
    is_active: true,
    wallet_type: 'descriptor',
  }

  const contacts: Contact[] = [
    {
      id: 'contact-1',
      wallet_checksum: 'test-checksum',
      name: 'Alice',
      notification_methods: [],
      created_at: '2024-01-01T00:00:00Z',
      is_active: true,
    },
    {
      id: 'contact-2',
      wallet_checksum: 'test-checksum',
      name: 'Bob',
      notification_methods: [],
      created_at: '2024-01-02T00:00:00Z',
      is_active: true,
    },
  ]

  beforeEach(() => {
    mockUseAuth.mockReturnValue({
      isCloudMode: true,
      billingStatus: {
        limits: {
          max_contacts_per_wallet: 5,
        },
      },
    })
  })

  it('shows contact usage when cloud limits are finite', () => {
    render(
      <WalletInfoSidebar
        wallet={wallet}
        contacts={contacts}
        balanceAlerts={[]}
        onAddContact={jest.fn()}
        onContactsUpdated={jest.fn()}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.getByText('2 / 5')).toBeInTheDocument()
    expect(screen.getByLabelText('2 of 5 contacts used')).toBeInTheDocument()
  })

  it('hides contact usage when limits are disabled', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: false,
      billingStatus: {
        limits: {
          max_contacts_per_wallet: -1,
        },
      },
    })

    render(
      <WalletInfoSidebar
        wallet={wallet}
        contacts={contacts}
        balanceAlerts={[]}
        onAddContact={jest.fn()}
        onContactsUpdated={jest.fn()}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.queryByText('2 / -1')).not.toBeInTheDocument()
    expect(screen.queryByLabelText(/contacts used/)).not.toBeInTheDocument()
  })

  it('hides contact usage when billing limits are unavailable', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: true,
      billingStatus: {
        subscription_status: 'active',
      },
    })

    render(
      <WalletInfoSidebar
        wallet={wallet}
        contacts={contacts}
        balanceAlerts={[]}
        onAddContact={jest.fn()}
        onContactsUpdated={jest.fn()}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.queryByLabelText(/contacts used/)).not.toBeInTheDocument()
  })

  it('does not highlight contact usage at the limit', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: true,
      billingStatus: {
        limits: {
          max_contacts_per_wallet: 2,
        },
      },
    })

    render(
      <WalletInfoSidebar
        wallet={wallet}
        contacts={contacts}
        balanceAlerts={[]}
        onAddContact={jest.fn()}
        onContactsUpdated={jest.fn()}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.getByText('2 / 2')).not.toHaveClass('text-orange-700')
  })

  it('highlights contact usage when over the limit', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: true,
      billingStatus: {
        limits: {
          max_contacts_per_wallet: 1,
        },
      },
    })

    render(
      <WalletInfoSidebar
        wallet={wallet}
        contacts={contacts}
        balanceAlerts={[]}
        onAddContact={jest.fn()}
        onContactsUpdated={jest.fn()}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.getByText('2 / 1')).toHaveClass('text-orange-700')
  })
})
