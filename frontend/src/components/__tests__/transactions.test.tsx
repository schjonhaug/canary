import React from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Transactions } from '../transactions'
import { NotificationStatus, Transaction } from '../../types'

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

    if (key === 'loadOlder') {
      return 'Load older'
    }

    if (key === 'expandDetails') {
      return 'Expand transaction details'
    }

    if (key === 'collapseDetails') {
      return 'Collapse transaction details'
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
  TransactionDetails: ({
    transaction,
    notifications,
  }: {
    transaction: Transaction
    notifications?: NotificationStatus[]
  }) => {
    const providerTypes = notifications?.map(({ provider_type }) => provider_type).join(',') ?? 'none'

    return <div data-testid={`details-${transaction.txid}`}>{providerTypes}</div>
  },
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
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    expect(screen.queryByTestId(`details-${transactions[0].txid}`)).not.toBeInTheDocument()

    const expandButton = screen.getByRole('button', { name: 'Expand transaction details' })
    expect(expandButton).toHaveAttribute('aria-expanded', 'false')
    expect(expandButton).toHaveAttribute(
      'aria-controls',
      `transaction-details-${transactions[0].wallet_checksum}:${transactions[0].txid}`,
    )

    fireEvent.click(expandButton)

    expect(expandButton).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByTestId(`details-${transactions[0].txid}`)).toBeInTheDocument()
  })

  it('supports keyboard expansion on the desktop toggle button', async () => {
    const user = userEvent.setup()

    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    const expandButton = screen.getByRole('button', { name: 'Expand transaction details' })
    expandButton.focus()

    await user.keyboard('{Enter}')

    expect(screen.getByRole('button', { name: 'Collapse transaction details' })).toHaveAttribute(
      'aria-expanded',
      'true',
    )
    expect(screen.getByTestId(`details-${transactions[0].txid}`)).toBeInTheDocument()
  })

  it('expands when the desktop row is clicked', () => {
    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    fireEvent.click(screen.getByText('1000'))

    expect(screen.getByTestId(`details-${transactions[0].txid}`)).toBeInTheDocument()
  })

  it('calls load more when older history is requested', () => {
    const onLoadMore = jest.fn()

    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
        hasMoreTransactions
        onLoadMore={onLoadMore}
      />,
    )

    fireEvent.click(screen.getAllByRole('button', { name: 'Load older' })[0])

    expect(onLoadMore).toHaveBeenCalledTimes(1)
  })

  it('uses the earliest transaction timestamp in the desktop row', () => {
    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    expect(screen.getByText('1000')).toBeInTheDocument()
    expect(screen.queryByText('1001')).not.toBeInTheDocument()
  })

  it('shows notification details only after expanding the desktop row', async () => {
    const user = userEvent.setup()
    const rowNotifications: NotificationStatus[] = [
      {
        contact_name: 'Alice',
        provider_name: 'email',
        provider_type: 'email',
        notification_type: 'confirmed',
        status: 'sent',
        notification_target: 'alice@example.com',
        error_message: null,
        created_at: '1001',
      },
      {
        contact_name: 'Bob',
        provider_name: 'ntfy',
        provider_type: 'ntfy',
        notification_type: 'confirmed',
        status: 'sent',
        notification_target: 'canary-topic',
        error_message: null,
        created_at: '1001',
      },
    ]

    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
        transactionNotifications={{
          [`${transactions[0].wallet_checksum}:${transactions[0].txid}`]: rowNotifications,
        }}
      />,
    )

    expect(screen.queryByText('email,ntfy')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Expand transaction details' }))

    expect(screen.getByTestId(`details-${transactions[0].txid}`)).toHaveTextContent('email,ntfy')
  })

  it('only auto-loads notifications once per expansion when props are unchanged', () => {
    const loadTransactionNotifications = jest.fn()

    const { rerender } = render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
        loadTransactionNotifications={loadTransactionNotifications}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Expand transaction details' }))

    expect(loadTransactionNotifications).toHaveBeenCalledTimes(1)
    expect(loadTransactionNotifications).toHaveBeenCalledWith(
      'wallet-1',
      transactions[0].txid,
    )

    rerender(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={transactions}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
        loadTransactionNotifications={loadTransactionNotifications}
      />,
    )

    expect(loadTransactionNotifications).toHaveBeenCalledTimes(1)
  })

  it('shows the selected-wallet empty state only once', () => {
    render(
      <Transactions
        selectedWalletChecksum="wallet-1"
        transactions={[]}
        error={null}
        lastUpdate={1001}
        walletsCount={1}
      />,
    )

    expect(screen.getAllByText('emptyForWallet')).toHaveLength(1)
    expect(screen.queryByText('empty')).not.toBeInTheDocument()
  })

  it('shows the all-wallets empty state only once', () => {
    render(
      <Transactions
        transactions={[]}
        error={null}
        lastUpdate={1001}
        walletsCount={0}
      />,
    )

    expect(screen.getAllByText('empty')).toHaveLength(1)
    expect(screen.queryByText('emptyForWallet')).not.toBeInTheDocument()
  })
})
