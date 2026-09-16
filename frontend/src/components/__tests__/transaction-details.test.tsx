import React from 'react'
import { render, screen } from '@testing-library/react'
import { TransactionDetails } from '../transaction-details'
import { Transaction } from '../../types'

jest.mock('next-intl', () => ({
  useTranslations: () => (key: string) => key,
}))

jest.mock('@/hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatTransactionAmount: (amount: number) => `${amount} sats`,
    formatDateTime: (value: number | string) => String(value),
  }),
}))

jest.mock('@/hooks/useTxExplorer', () => ({
  useTxExplorer: () => ({
    name: 'Mempool',
    baseUrl: 'https://mempool.space',
  }),
}))

const transaction: Transaction = {
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
  label: 'rent',
}

describe('TransactionDetails label editor', () => {
  it('hides the label editor when onLabelChange is omitted', () => {
    render(<TransactionDetails transaction={transaction} isExpanded />)

    expect(screen.queryByLabelText('label:')).not.toBeInTheDocument()
  })

  it('shows the label editor when onLabelChange is provided', () => {
    render(
      <TransactionDetails
        transaction={transaction}
        isExpanded
        onLabelChange={async () => undefined}
      />
    )

    expect(screen.getByLabelText('label:')).toBeInTheDocument()
  })
})
