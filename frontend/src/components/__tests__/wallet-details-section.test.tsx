import React from 'react'
import { render, screen, fireEvent } from '@testing-library/react'
import { WalletDetailsSection } from '../wallet-detail/wallet-details-section'
import type { Wallet } from '../../types'

jest.mock('../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatDateTime: (value: string | number) => String(value),
  }),
}))

jest.mock('../../hooks/useRelativeTime', () => ({
  useRelativeTime: () => undefined,
}))

describe('WalletDetailsSection', () => {
  const originalClipboard = navigator.clipboard
  const wallet: Wallet = {
    checksum: 'wallet-1',
    name: 'Test Wallet',
    descriptor: 'addr(bc1qexampleaddress)',
    wallet_filename: 'test-wallet',
    created_at: '2024-01-01T00:00:00Z',
    balance_total: 0,
    last_activity: null,
    status: 'ready',
    contact_count: 0,
    is_active: true,
    wallet_type: 'address',
  }

  beforeEach(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: jest.fn().mockResolvedValue(undefined),
      },
    })
  })

  afterEach(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: originalClipboard,
    })
  })

  it('labels the icon-only copy button', () => {
    render(<WalletDetailsSection wallet={wallet} />)

    fireEvent.click(screen.getByRole('button', { name: 'Wallet Details' }))

    expect(screen.getByRole('button', { name: 'Copy' })).toBeInTheDocument()
  })

  it('updates the copy button label after copying', async () => {
    render(<WalletDetailsSection wallet={wallet} />)

    fireEvent.click(screen.getByRole('button', { name: 'Wallet Details' }))
    fireEvent.click(screen.getByRole('button', { name: 'Copy' }))

    expect(await screen.findByRole('button', { name: 'Copied!' })).toBeInTheDocument()
  })
})
