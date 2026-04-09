import React from 'react'
import { render, screen, fireEvent } from '@testing-library/react'
import { BalanceAlertsList } from '../balance-alerts-list'
import type { BalanceAlert } from '../../types'

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => ({
    user: { is_admin: false, is_demo: false },
    isCloudMode: false,
  }),
}))

jest.mock('../../lib/api', () => ({
  ApiError: class ApiError extends Error {},
  api: {
    getUserPreferences: jest.fn().mockResolvedValue({ preferred_fiat_currency: 'USD' }),
    deleteBalanceAlert: jest.fn().mockResolvedValue({}),
    createBalanceAlert: jest.fn(),
  },
}))

jest.mock('../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatFiatAmount: (value: number, currency: string) => `${value} ${currency}`,
    formatBtcAmount: (value: number) => String(value),
  }),
}))

describe('BalanceAlertsList', () => {
  const alert: BalanceAlert = {
    id: 'alert-1',
    wallet_checksum: 'wallet-1',
    threshold_sats: 100000,
    alert_type: 'below',
    is_active: true,
    created_at: '2024-01-01T00:00:00Z',
  }

  it('labels the icon-only delete alert button', () => {
    render(<BalanceAlertsList walletChecksum="wallet-1" balanceAlerts={[alert]} />)

    expect(screen.getByRole('button', { name: 'Delete alert' })).toBeInTheDocument()
  })

  it('labels the icon-only close button in the create form', () => {
    render(<BalanceAlertsList walletChecksum="wallet-1" balanceAlerts={[]} />)

    fireEvent.click(screen.getByRole('button', { name: 'New' }))

    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument()
  })
})
