import { isRecoverableWallet, isStalePendingWallet } from '../wallet-status'
import type { Wallet } from '@/types'

const baseWallet: Wallet = {
  checksum: 'wallet-1',
  name: 'Test Wallet',
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
}

describe('wallet status helpers', () => {
  it('does not treat fresh pending wallets as stale', () => {
    expect(isStalePendingWallet(baseWallet, Date.parse('2026-06-02T10:09:59Z'))).toBe(false)
  })

  it('treats pending wallets older than 10 minutes with no sync as stale', () => {
    expect(isStalePendingWallet(baseWallet, Date.parse('2026-06-02T10:10:00Z'))).toBe(true)
  })

  it('does not treat synced pending wallets as stale', () => {
    expect(
      isStalePendingWallet(
        { ...baseWallet, last_synced_at: '2026-06-02 10:05:00' },
        Date.parse('2026-06-02T10:30:00Z')
      )
    ).toBe(false)
  })

  it('treats failed wallets as recoverable immediately', () => {
    expect(isRecoverableWallet({ ...baseWallet, status: 'failed' })).toBe(true)
  })
})
