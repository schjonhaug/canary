import React from 'react'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { getWalletDetailErrorState } from '../wallet-detail-error-states'
import type { Wallet } from '@/types'

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

const translations: Record<string, string> = {
  'detail.failed.title': 'Sync Failed',
  'detail.failed.description': 'Wallet {name} failed to sync.',
  'detail.stuck.title': 'Sync Stuck',
  'detail.stuck.description': 'Wallet {name} appears stuck.',
  'detail.syncing.title': 'Syncing',
  'detail.syncing.description': 'Wallet {name} is syncing.',
  'detail.syncing.returnPrompt': 'Come back soon.',
}

const commonTranslations: Record<string, string> = {
  backToWallets: 'Back to wallets',
  delete: 'Delete',
  deleting: 'Deleting...',
}

const t = (key: string, params?: Record<string, string>) => {
  const value = translations[key] ?? key
  return params?.name ? value.replace('{name}', params.name) : value
}

const tCommon = (key: string) => commonTranslations[key] ?? key

const renderErrorState = (walletOverride: Partial<Wallet>, options = {}) => {
  const node = getWalletDetailErrorState({
    error: null,
    wallet: wallet(walletOverride),
    checksum: walletOverride.checksum ?? 'wallet-1',
    t,
    tCommon,
    ...options,
  })

  render(<>{node}</>)
}

describe('getWalletDetailErrorState recovery states', () => {
  it('renders failed wallet recovery with delete action', async () => {
    const onDeleteWallet = jest.fn()
    const user = userEvent.setup()

    renderErrorState(
      { status: 'failed', created_at: '2026-06-02 10:09:59' },
      { canDelete: true, onDeleteWallet, now: Date.parse('2026-06-02T10:10:00Z') }
    )

    expect(screen.getByText('Sync Failed')).toBeInTheDocument()
    expect(screen.getByText('Wallet Wallet 1 failed to sync.')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /delete/i }))

    expect(onDeleteWallet).toHaveBeenCalledTimes(1)
  })

  it('renders stale pending recovery with delete action', () => {
    renderErrorState(
      { status: 'pending', created_at: '2026-06-02 10:00:00' },
      {
        canDelete: true,
        onDeleteWallet: jest.fn(),
        now: Date.parse('2026-06-02T10:10:00Z'),
      }
    )

    expect(screen.getByText('Sync Stuck')).toBeInTheDocument()
    expect(screen.getByText('Wallet Wallet 1 appears stuck.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /delete/i })).toBeInTheDocument()
  })

  it('keeps fresh pending wallets in the syncing state', () => {
    renderErrorState(
      { status: 'pending', created_at: '2026-06-02 10:05:00' },
      { now: Date.parse('2026-06-02T10:10:00Z') }
    )

    expect(screen.getByText('Syncing')).toBeInTheDocument()
    expect(screen.queryByText('Sync Stuck')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })
})
