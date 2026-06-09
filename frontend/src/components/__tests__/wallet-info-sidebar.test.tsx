import React from 'react'
import { render, screen } from '@testing-library/react'
import { WalletInfoSidebar } from '../wallet-detail/wallet-info-sidebar'
import type { Wallet } from '../../types'

const walletDetailsSectionMock = jest.fn()

jest.mock('../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatBitcoinAmount: (value: number) => `${value} sats`,
    formatFiatAmount: (value: number, currency: string) => `${value} ${currency}`,
  }),
}))

jest.mock('../wallet-detail/wallet-details-section', () => ({
  WalletDetailsSection: (props: unknown) => {
    walletDetailsSectionMock(props)
    return <div data-testid="wallet-details-section" />
  },
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
    balance_fiat: 12.34,
    fiat_currency: 'USD',
    last_activity: null,
    status: 'ready',
    contact_count: 2,
    is_active: true,
    wallet_type: 'descriptor',
  }

  beforeEach(() => {
    walletDetailsSectionMock.mockClear()
  })

  it('shows the wallet balance and details section', () => {
    render(
      <WalletInfoSidebar
        wallet={wallet}
        onDeleteClick={jest.fn()}
        showActions
      />
    )

    expect(screen.getByText('1000 sats')).toBeInTheDocument()
    expect(screen.getByText('12.34 USD')).toBeInTheDocument()
    expect(screen.getByTestId('wallet-details-section')).toBeInTheDocument()
  })

  it('passes the delete action through when actions are visible', () => {
    const onDeleteClick = jest.fn()

    render(
      <WalletInfoSidebar
        wallet={wallet}
        onDeleteClick={onDeleteClick}
        showActions
      />
    )

    expect(walletDetailsSectionMock).toHaveBeenCalledWith(
      expect.objectContaining({ wallet, onDeleteClick })
    )
  })

  it('hides the delete action when actions are disabled', () => {
    render(
      <WalletInfoSidebar
        wallet={wallet}
        onDeleteClick={jest.fn()}
        showActions={false}
      />
    )

    expect(walletDetailsSectionMock).toHaveBeenCalledWith(
      expect.objectContaining({ wallet, onDeleteClick: undefined })
    )
  })
})
