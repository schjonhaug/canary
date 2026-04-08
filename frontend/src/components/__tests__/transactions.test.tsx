import React from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { Transactions } from '../transactions'
import { Transaction } from '../../types'

jest.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 74,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        start: index * 74,
      })),
    measureElement: jest.fn(),
  }),
}))

jest.mock('next-intl', () => ({
  useTranslations: (namespace: string) => (key: string, values?: Record<string, string | number>) => {
    if (namespace === 'common' && key === 'loading') {
      return 'Loading'
    }

    if (key === 'titleWithWallet') {
      return `Transactions - ${values?.walletName ?? ''}`
    }

    if (key === 'count') {
      return `${values?.count ?? 0} transactions`
    }

    return key
  },
}))

jest.mock('@/hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatTransactionAmount: (amount: number) => `${amount} sats`,
    formatDateTime: (value: number | string) => String(value),
  }),
}))

jest.mock('../transaction-card', () => ({
  TransactionCard: ({ transaction }: { transaction: Transaction }) => (
    <div data-testid={`mobile-${transaction.txid}`}>{transaction.txid}</div>
  ),
}))

jest.mock('../transaction-details', () => ({
  TransactionDetails: ({ transaction }: { transaction: Transaction }) => (
    <div data-testid={`details-${transaction.txid}`}>details</div>
  ),
}))

describe('Transactions', () => {
  const transactions: Transaction[] = [
    {
      txid: 'a'.repeat(64),
      wallet_checksum: 'wallet-1',
      wallet_name: 'Primary wallet',
      transaction_type: 'receive',
      amount_sats: 1234,
      fee_sats: null,
      block_height: 1,
      first_seen_at: 1000,
      confirmed_at: 1001,
      parent_txid: null,
      transaction_status: 'confirmed',
      replaced_by_txid: null,
      replaced_at: null,
      notification_status: [],
    },
  ]

  it('sets accessible expand state and only renders details after expanding', () => {
    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        isConnected
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    expect(screen.queryByTestId(`details-${transactions[0].txid}`)).not.toBeInTheDocument()

    const expandButton = screen.getByRole('button')
    expect(expandButton).toHaveAttribute('aria-expanded', 'false')
    expect(expandButton).toHaveAttribute(
      'aria-controls',
      `transaction-details-${transactions[0].txid}`,
    )

    fireEvent.click(expandButton)

    expect(expandButton).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByTestId(`details-${transactions[0].txid}`)).toBeInTheDocument()
  })
})
