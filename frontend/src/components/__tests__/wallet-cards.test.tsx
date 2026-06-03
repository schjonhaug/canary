import React from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WalletCards } from '../wallet-cards'
import { api, ApiError } from '@/lib/api'
import type { Wallet } from '@/types'

jest.mock('@/lib/api', () => ({
  ...jest.requireActual('@/lib/api'),
  api: {
    ...jest.requireActual('@/lib/api').api,
    deleteWallet: jest.fn(),
  },
}))

jest.mock('@/lib/utils', () => {
  const actual = jest.requireActual('@/lib/utils')
  return {
    ...actual,
    getCachedWalletSvg: jest.fn(() => '<svg />'),
    loadWalletSvg: jest.fn(() => Promise.resolve('<svg />')),
    formatDateTime: jest.fn((value: string | number) => String(value)),
  }
})

jest.mock('@/hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatBitcoinAmount: (value: number) => `${value} sats`,
    formatFiatAmount: (value: number, currency: string) => `${value} ${currency}`,
    locale: 'en-US',
  }),
}))

const wallet = (overrides: Partial<Wallet>): Wallet => ({
  checksum: 'wallet-1',
  name: 'Wallet 1',
  descriptor: 'wpkh(test)#abc123',
  wallet_filename: 'wallet-1',
  hex_color: '#f59e0b',
  created_at: '2026-06-02 10:00:00',
  balance_total: 0,
  last_activity: null,
  status: 'pending',
  contact_count: 0,
  is_active: true,
  wallet_type: 'descriptor',
  last_synced_at: null,
  ...overrides,
})

describe('WalletCards recovery states', () => {
  beforeEach(() => {
    jest.useFakeTimers().setSystemTime(new Date('2026-06-02T10:11:00Z'))
    jest.mocked(api.deleteWallet).mockResolvedValue(undefined)
  })

  afterEach(() => {
    jest.useRealTimers()
    jest.clearAllMocks()
  })

  it('renders normal pending sync UI before the stale threshold', () => {
    render(
      <WalletCards
        wallets={[wallet({ created_at: '2026-06-02 10:05:00' })]}
        error={null}
        lastUpdate={1}
      />
    )

    expect(screen.getByText('Syncing...')).toBeInTheDocument()
    expect(screen.queryByText('Sync Stuck')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })

  it('renders stale pending recovery UI and deletes through the API', async () => {
    const onWalletDeleted = jest.fn()

    render(
      <WalletCards
        wallets={[wallet({ checksum: 'stale-wallet' })]}
        error={null}
        lastUpdate={1}
        onWalletDeleted={onWalletDeleted}
      />
    )

    expect(screen.getByText('Sync Stuck')).toBeInTheDocument()

    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime })
    await user.click(screen.getByRole('button', { name: /delete/i }))

    await waitFor(() => {
      expect(api.deleteWallet).toHaveBeenCalledWith('stale-wallet')
      expect(onWalletDeleted).toHaveBeenCalled()
    })
  })

  it('renders failed recovery UI immediately', () => {
    render(
      <WalletCards
        wallets={[wallet({ status: 'failed', created_at: '2026-06-02 10:10:59' })]}
        error={null}
        lastUpdate={1}
      />
    )

    expect(screen.getByText('Sync Failed')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /delete/i })).toBeInTheDocument()
  })

  it('shows an error when deleting a recoverable wallet fails', async () => {
    jest
      .mocked(api.deleteWallet)
      .mockRejectedValueOnce(new ApiError('Delete failed', 'server', 500))
    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime })

    render(
      <WalletCards
        wallets={[wallet({ checksum: 'failed-wallet', status: 'failed' })]}
        error={null}
        lastUpdate={1}
      />
    )

    await user.click(screen.getByRole('button', { name: /delete/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Something went wrong on our end. Please try again later.'
    )
  })
})
